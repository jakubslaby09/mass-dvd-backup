use std::{ffi::OsStr, collections::HashMap, fs::create_dir_all, path::{Path, PathBuf}, process::{Stdio, Command}};

use crate::{VIDEO_EXTENSION, FFMPEG};

pub struct TrackMap {
    tracks: HashMap<String, Track>
}

impl TrackMap {
    pub fn new() -> Self {
        TrackMap { tracks: HashMap::new() }
    }

    pub fn push(&mut self, path: PathBuf) {
        // TODO: insert instead of pushig
        // TODO: don't clone
        let track_name = track_name(&path);
        self.tracks.entry(track_name.clone()).and_modify(
            |track| track.append(path.clone())
        ).or_insert(Track::new(path));
    }

    fn take_largest(&mut self) -> Option<(String, Track)> {
        // TODO: don't iterate twice or clone
        let largest = self.tracks.iter().max_by_key(|track| track.1.bytes)?.0.clone();
        self.tracks.remove_entry(&largest)
    }

    pub fn backup(mut self, dvd_name: String, out_dir: &Path) {
        let largest = match self.take_largest() {
            Some(it) => it,
            None => {
                eprintln!("there is no main track to compress");
                return;
            },
        };
        
        largest.1.backup(dvd_name, out_dir, VIDEO_EXTENSION);
        for track in self.tracks {
            track.1.backup(track.0, out_dir, VIDEO_EXTENSION);
        }
    }
}

struct Track {
    bytes: u64,
    files: Vec<PathBuf>,
}

impl Track {
    fn new(first: PathBuf) -> Self {
        let mut track = Track { bytes: 0, files: vec![] };
        track.append(first);
        track
    }

    fn append(&mut self, path: PathBuf) {
        match path.metadata() {
            Ok(it) => self.bytes += it.len(),
            Err(err) => eprintln!("couldn't get size of video file {}: {err}", path.to_string_lossy()),
        }
        self.files.push(path);
    }

    fn backup(self, name: String, out_dir: &Path, extension: &str) {
        let mut output = out_dir.join(&name);
        output.set_extension(extension);
        
        if let Err(err) = create_dir_all(out_dir) {
            eprintln!("could not create a directory for track {}: {err}", name);
        }
        println!("compressing track {}", name);

        let input_args: Vec<&OsStr> = self.files.iter().map(
            |file| [OsStr::new("-i"), file.as_os_str()]
        ).flatten().collect();
        
        let concat_map: String = self.files.iter().enumerate().map(
            |(index, _)| format!("[{index}:v][{index}:a]")
        ).collect();
        
        // println!("filter arg: {concat_map}concat=n={count}:v=1:a=1[v][a]", count = self.files.len());
        
        /* let mut child = match FfmpegCommand::new()
        .input(input)
        .output(output)
        .pipe_stdout()
        .spawn() {
            Ok(it) => it,
            Err(error) => {
                eprintln!("there was an error when spawning ffmpeg: {error}");
                continue;
            }
        }; */
        // TODO: use ffmpeg-sidecar
        let status = match Command::new(FFMPEG)
        .args(input_args)
        .args(["-filter_complex", &format!("{concat_map}concat=n={count}:v=1:a=1[v][a]", count = self.files.len())])
        .args(["-map", "[v]", "-map", "[a]"])
        .args(["-vcodec", "h264_nvenc", "-preset", "medium"])
        .arg(output)
        .arg("-hide_banner")
        .stdin(Stdio::null())
        .status() {
            Ok(it) => it,
            Err(error) => {
                eprintln!("there was an error when spawning ffmpeg: {error}");
                return;
            }
        };

        if !status.success() {
            eprintln!("ffmpeg returned {status}");
            // if let Some(out) = child.take_stdout() {
            //     write_log(out);
            // }
            return;
        }
        /* match status {
            Ok(status) if !status.success() => {
                eprintln!("ffmpeg returned {status}");
                // if let Some(out) = child.take_stdout() {
                //     write_log(out);
                // }
                continue;
            },
            Ok(_) => (),
            Err(error) => {
                eprintln!("there was an error when running ffmpeg: {error}");
                continue;
            }
        } */
    }
}

fn track_name(path: &Path) -> String {
    // TODO: don't use lossy conversion
    let stem = path.file_stem().expect("we use push only for files").to_string_lossy();
    match stem.rsplit_once("_") {
        Some(it) => it.0.to_string(),
        None => {
            eprintln!("video file {} doesn't have any underscores in it's name", path.to_string_lossy());
            stem.to_string()
        },
    }
}