use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use std::io::{Read, Write};

const CHUNK_SIZE: usize = 4096;

/// Display an image using the Kitty graphics protocol (transmit + display at cursor).
fn display_image(data: &[u8]) -> Result<()> {
    let img = image::load_from_memory(data)?;
    let mut png_data = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut png_data);
    img.write_to(&mut cursor, image::ImageFormat::Png)?;
    let encoded = BASE64.encode(&png_data);

    let mut stdout = std::io::stdout().lock();

    let chunks: Vec<&str> = encoded
        .as_bytes()
        .chunks(CHUNK_SIZE)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect();

    for (i, chunk) in chunks.iter().enumerate() {
        let is_last = i == chunks.len() - 1;
        let m = if is_last { 0 } else { 1 };
        if i == 0 {
            write!(stdout, "\x1b_Ga=T,f=100,t=d,q=2,m={m};{chunk}\x1b\\")?;
        } else {
            write!(stdout, "\x1b_Gq=2,m={m};{chunk}\x1b\\")?;
        }
    }
    stdout.flush()?;
    Ok(())
}

/// Display image with a message and wait for keypress.
pub fn show_image_fullscreen(data: &[u8], filename: &str) -> Result<()> {
    let mut stdout = std::io::stdout().lock();

    // Clear screen and move to top
    write!(stdout, "\x1b[2J\x1b[H")?;
    writeln!(stdout, "Image: {filename}\n")?;
    stdout.flush()?;

    display_image(data)?;

    writeln!(stdout, "\n\nPress any key to continue...")?;
    stdout.flush()?;

    // Wait for a keypress (raw mode should already be disabled by caller)
    let mut buf = [0u8; 1];
    std::io::stdin().read_exact(&mut buf)?;

    Ok(())
}
