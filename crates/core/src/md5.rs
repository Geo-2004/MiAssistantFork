use md5;
use std::io::{Read};
use std::fs::File;
use crate::errors::Result;

pub fn md5_file(path: &str) -> Result<String> {
    let mut file = File::open(path)?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    let digest = md5::compute(&data);
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
