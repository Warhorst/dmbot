use std::process::Command;

pub fn get_video_name(url: &str) -> Result<String, String> {
    let output = Command::new("yt-dlp")
        .arg("--get-title")
        .arg(url)
        .output().unwrap();

    Ok(String::from_utf8(output.stdout).unwrap())
}