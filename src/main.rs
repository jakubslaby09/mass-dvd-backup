mod tracks;

use std::{process::exit, error::Error, fs::{read_dir, create_dir_all}, path::Path, ffi::OsStr, thread::sleep, time::Duration, io::{stdin, stdout, Write}};

use eject::{discovery::cd_drives, device::{Device, DriveStatus}};
use sys_mount::{UnmountFlags, MountFlags};

use crate::tracks::TrackMap;

const TMP_DIR: &str = "/tmp/mass-dvd-backup";
const OUT_DIR: &str = "/media/data/Videa/mass-dvd-backup";
const FFMPEG: &str = "ffmpeg";
// const FFMPEG_LOG: &Path = &Path::new("./mass-dvd-backup-ffmpeg.log");

const DRIVE_CHECK_INTERVAL: Duration = Duration::from_secs(3);
const DEFAULT_DRIVE: &str = "/dev/cdrom";
const VIDEO_EXTENSION: &str = "mp4";

fn main() {
    let use_ffmpeg = ffmpeg_sidecar::version::ffmpeg_version().is_ok();
    if !use_ffmpeg {
        eprintln!("cannot get ffmpeg version. without ffmpeg, no files will be compressed");
    }

    let mut first_eject_check = true;
    loop {
        let cdrom_path = OsStr::new(DEFAULT_DRIVE);/* match cd_drives().next() {
            Some(it) => it,
            None => {
                eprintln!("no drive connected!");
                exit(1);
            },
        }; */
        
        let cdrom = match Device::open(&cdrom_path) {
            Ok(it) => it,
            Err(err) => {
                eprintln!("couldn't access drive {}\n{err}", cdrom_path.to_string_lossy());
                exit(1);
            },
        };
        let mut eject = true;
        if first_eject_check {
            first_eject_check = false;
            eject = match cdrom.status() {
                Ok(status) => {
                    match status {
                        DriveStatus::TrayOpen |
                        DriveStatus::Loaded => false,
                        _ => true,
                    }
                }
                _ => true,
            };
        }

        
        if eject {
            match cdrom.eject() {
                Ok(_) => (),
                Err(e) => eprintln!("couldn't eject cdrom: {e}"),
            }
        }
        
        println!("waiting for drive {} to be closed", cdrom_path.to_string_lossy());
        match wait_for_tray(&cdrom) {
            Ok(DriveStatus::Loaded) => (),
            Ok(_) => {
                eprintln!("the drive {} is empty", cdrom_path.to_string_lossy());
                continue;
            },
            Err(err) => {
                eprintln!("couldn't check the state of drive {}: {err}", cdrom_path.to_string_lossy());
                exit(1);
            },
        };
        // TODO: default to dvd label
        print!("choose an output name: ");
        let _ = stdout().flush();
        let mut name = String::new();
        match stdin().read_line(&mut name) {
            Ok(_) => (),
            Err(err) => {
                eprintln!("\rcould not read the output name: {err}");
                exit(1);
            },
        }
        name.pop();

        let _cdrom_lock =  match cdrom.lock_ejection() {
            Ok(it) => Some(it),
            Err(err) => {
                eprintln!("couldn't lock drive {}. Be careful to not eject it! {err}", cdrom_path.to_string_lossy());
                None
            }
        };

        backup_drive(&cdrom, &Path::new(OUT_DIR).join(&name), &name);
    }
}

fn backup_drive(cdrom: &Device, out_dir: &Path, dvd_name: &str) {
    // let device = unsafe { File::from_raw_fd(cdrom.as_fd()) };
    create_dir_all(TMP_DIR).expect(&format!("could not create temporary directory {TMP_DIR}"));
    // sys_mount::Mount::new(unsafe { File::from_raw_fd(cdrom.as_fd()) }, TMP_DIR);
    // TODO: offer a mode without mounting
    if let Ok(()) = sys_mount::unmount(&DEFAULT_DRIVE, UnmountFlags::empty()) {
        println!("unmounted drive {DEFAULT_DRIVE}");
    }
    let mount = sys_mount::Mount::builder()
    .flags(MountFlags::RDONLY)
    .mount(&DEFAULT_DRIVE, TMP_DIR)
    .expect(&format!(
        "could not mount drive {DEFAULT_DRIVE} to temporary directory {TMP_DIR}",
    ));

    backup_dir(mount.target_path(), out_dir, out_dir, dvd_name);

    sys_mount::unmount(mount.target_path(), UnmountFlags::empty()).expect(&format!(
        "could not unmount drive {DEFAULT_DRIVE}"
    ));
}

fn backup_dir(input_path: &Path, out_dir: &Path, out_video_dir: &Path, dvd_name: &str) {
    let files = read_dir(input_path).expect(&format!(
        "could not read temporary directory {TMP_DIR}"
    ));

    let mut track_map = TrackMap::new();

    for input_file in files {
        let input_filename = match input_file {
            Ok(it) => it,
            Err(err) => {
                eprintln!("could not read a file from temporary directory {TMP_DIR}, skipping it: {err}");
                continue;
            },
        };

        let input = input_path.join(input_filename.file_name());
        
        let output = out_dir.join(input_filename.file_name());
        
        // TODO: don't use if
        if input_filename.path().extension().unwrap_or_default().to_ascii_lowercase() == "vob" {
            track_map.push(input);
            println!("added {}", input_filename.file_name().to_string_lossy());
        } else if input.is_dir() {
            println!("backing up subdirectory {}", input.to_string_lossy());
            backup_dir(&input, &output, out_video_dir, &dvd_name);
            println!("done subdirectory {}", input.to_string_lossy());
        } else {
            create_dir_all(output.parent().unwrap()).unwrap();
            if let Err(error) = std::fs::copy(input, output) {
                eprintln!("could not copy file {}: {error}", input_filename.path().to_string_lossy());
                continue;
            }
            println!("copied {}", input_filename.file_name().to_string_lossy());
        }
    }

    track_map.backup(dvd_name.to_string(), out_video_dir);
}

// fn write_log(out: ChildStdout) -> Result<(), std::io::Error> {
//     create_dir_all(FFMPEG_LOG)?;
//     let out_file = unsafe {
//         File::from_raw_fd(out.as_raw_fd())
//     };

    

//     std::fs::copy(out_file, FFMPEG_LOG)?;

//     Ok(())
// }

// fn mount_drive(cdrom: &Device) -> Result<PathBuf, Box<dyn Error>> {

//     todo!()
// }

fn wait_for_tray(cdrom: &Device) -> Result<DriveStatus, Box<dyn Error>> {
    let mut was_empty = false;
    Ok(loop {
        let status = cdrom.status()?;
        match status {
            DriveStatus::TrayOpen |
            DriveStatus::NotReady => (),
            DriveStatus::Empty if !was_empty => {
                was_empty = true;
            },
            DriveStatus::Empty |
            DriveStatus::Loaded => break status,
        }
        sleep(DRIVE_CHECK_INTERVAL);
    })
}