use crate::errors::Result;
use md5::Context;
use std::fs::File;
use std::io::Read;

pub fn md5_file(path: &str) -> Result<String> {
    let mut file = File::open(path)?;
    let mut ctx = Context::new();
    let mut buf = [0u8; 1024 * 1024]; // 1 MiB chunks
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        ctx.consume(&buf[..n]);
    }
    let digest = ctx.compute();
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
