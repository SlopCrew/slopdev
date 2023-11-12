use anyhow::Context;
use eframe::egui;
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::{atomic::AtomicUsize, Arc, Mutex},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DownloadStage {
    BepInEx,
    CheckingSlopCrew,
    DownloadingSlopCrew,
    ExtractingSlopCrew,
    Done,
}

#[derive(Debug, Clone)]
pub struct SlopDev {
    running: bool,
    work_dir: PathBuf,
    download_progress: Arc<AtomicUsize>,
    download_progress_max: Arc<AtomicUsize>,
    download_stage: Arc<Mutex<DownloadStage>>,
}

impl SlopDev {
    fn start_game(&mut self) {
        // TODO: bad assumption
        let steam_exe = "C:\\Program Files (x86)\\Steam\\Steam.exe";
        std::process::Command::new(steam_exe)
            .arg("-applaunch")
            .arg("1353230")
            .arg("--doorstop-enable")
            .arg("true")
            .arg("--doorstop-target")
            .arg(
                self.work_dir
                    .join("BepInEx/BepInEx/core/BepInEx.Preloader.dll")
                    .to_str()
                    .unwrap(),
            )
            .spawn()
            .expect("failed to start game");

        self.running = true;
    }
}

fn download_file(
    url: String,
    path: PathBuf,
    progress: Arc<AtomicUsize>,
    progress_max: Arc<AtomicUsize>,
) -> anyhow::Result<()> {
    let download = reqwest::blocking::get(url).context("failed to download file")?;
    let size = download
        .content_length()
        .context("failed to get file size")?;
    progress_max.store(size as usize, std::sync::atomic::Ordering::SeqCst);

    // delete old file
    std::fs::remove_file(&path).ok();

    let file = std::fs::File::create(path).context("failed to create file")?;
    let mut reader = std::io::BufReader::new(download);
    let mut writer = std::io::BufWriter::new(file);

    loop {
        let mut buf = [0; 1024];
        let amount = reader.read(&mut buf).context("failed to read download")?;

        if amount == 0 {
            break;
        }

        progress.fetch_add(amount, std::sync::atomic::Ordering::SeqCst);

        writer
            .write_all(&buf[..amount])
            .context("failed to write file")?;
    }

    Ok(())
}

fn unzip_zip(zip: std::fs::File, dir: PathBuf) -> anyhow::Result<()> {
    let mut archive = zip::ZipArchive::new(zip).context("failed to open zip")?;
    archive.extract(dir).context("failed to extract zip")?;
    Ok(())
}

impl SlopDev {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        let work_dir = directories::ProjectDirs::from("com", "notnite", "slopdev")
            .expect("failed to get project dirs")
            .data_dir()
            .to_owned();

        if !work_dir.exists() {
            std::fs::create_dir_all(&work_dir).expect("failed to create work dir");
        }

        let bepinex_dir = work_dir.join("BepInEx");
        let bepinex_exists = bepinex_dir.exists();
        let start_stage = if bepinex_exists {
            DownloadStage::CheckingSlopCrew
        } else {
            DownloadStage::BepInEx
        };

        let download_progress = Arc::new(AtomicUsize::new(0));
        let download_progress_max = Arc::new(AtomicUsize::new(0));
        let download_stage = Arc::new(Mutex::new(start_stage));

        let work_dir_clone = work_dir.clone();
        let download_progress_clone = download_progress.clone();
        let download_progress_max_clone = download_progress_max.clone();
        let download_stage_clone = download_stage.clone();

        std::thread::spawn(move || {
            if !bepinex_exists {
                let bepinex_url = "https://github.com/BepInEx/BepInEx/releases/download/v5.4.22/BepInEx_x64_5.4.22.0.zip";
                let bepinex_file = work_dir_clone.join("BepInEx.zip");

                download_file(
                    bepinex_url.to_owned(),
                    bepinex_file.clone(),
                    download_progress_clone.clone(),
                    download_progress_max_clone.clone(),
                )
                .expect("failed to download bepinex");

                unzip_zip(
                    std::fs::File::open(bepinex_file.clone()).expect("failed to open bepinex zip"),
                    bepinex_dir.clone(),
                )
                .expect("failed to unzip bepinex");

                std::fs::remove_file(bepinex_file).expect("failed to delete bepinex zip");

                std::fs::create_dir_all(bepinex_dir.join("BepInEx/config"))
                    .expect("failed to create config dir");
                let slop_config = r#"[Server]
Host = lmaobox.n2.pm
Port = 42069
"#;
                std::fs::write(
                    bepinex_dir.join("BepInEx/config/SlopCrew.Plugin.cfg"),
                    slop_config,
                )
                .expect("failed to write config");

                let bepinex_config = r#"[Logging.Console]
Enabled = true
"#;
                std::fs::write(
                    bepinex_dir.join("BepInEx/config/BepInEx.cfg"),
                    bepinex_config,
                )
                .expect("failed to write config");

                let mut download_stage_locked = download_stage_clone.lock().unwrap();
                *download_stage_locked = DownloadStage::CheckingSlopCrew;
                drop(download_stage_locked);
            }

            let timestamp_file = work_dir_clone.join("update.txt");
            let timestamp = if timestamp_file.exists() {
                std::fs::read_to_string(timestamp_file.clone())
                    .expect("failed to read timestamp file")
                    .trim()
                    .to_string()
            } else {
                "0".to_string()
            };
            let timestamp_number = timestamp.parse::<u64>().expect("failed to parse timestamp");

            let remote_update = reqwest::blocking::get("https://sloppers.club/dev/update.txt")
                .expect("failed to get remote update")
                .text()
                .expect("failed to get remote update text")
                .trim()
                .to_string();
            let remote_update_number = remote_update
                .parse::<u64>()
                .expect("failed to parse remote update");

            if remote_update_number > timestamp_number {
                let mut download_stage_locked = download_stage_clone.lock().unwrap();
                *download_stage_locked = DownloadStage::DownloadingSlopCrew;
                drop(download_stage_locked);

                let zip = work_dir_clone.join("SlopCrew.zip");
                download_file(
                    "https://sloppers.club/dev/SlopCrew.zip".to_string(),
                    zip.clone(),
                    download_progress_clone,
                    download_progress_max_clone,
                )
                .expect("failed to download slop crew");

                let mut download_stage_locked = download_stage_clone.lock().unwrap();
                *download_stage_locked = DownloadStage::ExtractingSlopCrew;
                drop(download_stage_locked);

                let slop_crew_dir = work_dir_clone.join("BepInEx/BepInEx/plugins/SlopCrew");
                std::fs::remove_dir_all(&slop_crew_dir).ok();
                unzip_zip(
                    std::fs::File::open(zip.clone()).expect("failed to open slop crew zip"),
                    slop_crew_dir.clone(),
                )
                .expect("failed to unzip slop crew");
                std::fs::remove_file(zip).expect("failed to delete slop crew zip");

                std::fs::write(timestamp_file, remote_update)
                    .expect("failed to write timestamp file");
            }

            let mut download_stage_locked = download_stage_clone.lock().unwrap();
            *download_stage_locked = DownloadStage::Done;
            drop(download_stage_locked);
        });

        Self {
            running: false,
            work_dir,
            download_progress,
            download_progress_max,
            download_stage,
        }
    }
}

impl eframe::App for SlopDev {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let download_stage = *self.download_stage.lock().unwrap();
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.running {
                ui.label("Started the game. You can close this window.");
            } else if download_stage == DownloadStage::Done {
                if ui.button("Start game").clicked() {
                    self.start_game();
                }
            } else {
                let download_progress = self
                    .download_progress
                    .load(std::sync::atomic::Ordering::SeqCst);
                let download_progress_max = self
                    .download_progress_max
                    .load(std::sync::atomic::Ordering::SeqCst);

                let download_progress_percentage =
                    download_progress as f32 / download_progress_max as f32;

                ui.add(
                    egui::ProgressBar::new(download_progress_percentage)
                        .show_percentage()
                        .text(match download_stage {
                            DownloadStage::BepInEx => "Downloading BepInEx",
                            DownloadStage::CheckingSlopCrew => "Checking for updates",
                            DownloadStage::DownloadingSlopCrew => "Downloading Slop Crew",
                            DownloadStage::ExtractingSlopCrew => "Extracting Slop Crew",
                            DownloadStage::Done => unreachable!(),
                        }),
                );
            }
        });
    }
}
