use md_5::{Md5, Digest};
use std::io::{Read};
use std::fs::File;
use crate::errors::Result;

pub fn md5_file(path: &str) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Md5::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?; if n == 0 { break; }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    Ok(format!("{:x}", digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn md5_known() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp, "hello world").unwrap();
        let sum = md5_file(tmp.path().to_str().unwrap()).unwrap();
        assert_eq!(sum, "5eb63bbbe01eeed093cb22bb8f5acdc3");
    }
}
