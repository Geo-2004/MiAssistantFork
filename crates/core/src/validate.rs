use crate::errors::{Result, Error};
use base64::{engine::general_purpose, Engine as _};
use serde::{Serialize, Deserialize};
use reqwest::blocking::Client;
use serde_json::Value;

#[derive(Serialize)]
struct Options { zone: String }
#[derive(Serialize)]
struct RequestPayload<'a> {
    d: &'a str,
    v: &'a str,
    c: &'a str,
    b: &'a str,
    sn: &'a str,
    l: &'a str,
    f: &'a str,
    options: Options,
    pkg: &'a str,
}

fn pkcs7_pad(mut data: Vec<u8>) -> Vec<u8> { let pad = 16 - (data.len() % 16); data.extend(std::iter::repeat(pad as u8).take(pad)); data }

fn aes128_cbc_encrypt(key: &[u8;16], iv: &[u8;16], plain: &[u8]) -> Result<Vec<u8>> {
    // Pure Rust: use aes + cbc crates, but to keep deps minimal we implement via openssl feature if enabled
    #[cfg(feature = "openssl")] {
        use openssl::symm::{Cipher, Crypter, Mode};
        let cipher = Cipher::aes_128_cbc();
        let mut c = Crypter::new(cipher, Mode::Encrypt, key, Some(iv)).map_err(|e| Error::Crypto(e.to_string()))?;
        let mut out = vec![0u8; plain.len() + cipher.block_size()];
        let mut count = c.update(plain, &mut out).map_err(|e| Error::Crypto(e.to_string()))?;
        count += c.finalize(&mut out[count..]).map_err(|e| Error::Crypto(e.to_string()))?;
        out.truncate(count); return Ok(out);
    }
    #[cfg(not(feature = "openssl"))]
    {
        use aes::Aes128; use cbc::cipher::{KeyIvInit, BlockEncryptMut, block_padding::Pkcs7};
        type Aes128CbcEnc = cbc::Encryptor<Aes128>;
        let enc = Aes128CbcEnc::new_from_slices(key, iv).map_err(|e| Error::Crypto(e.to_string()))?;
        let out = enc.encrypt_padded_vec_mut::<Pkcs7>(plain);
        Ok(out)
    }
}

#[derive(Debug, Deserialize)]
pub struct PackageRom {
    #[serde(rename = "Validate")]
    pub validate: Option<String>,
    #[serde(rename = "Erase")]
    pub erase: Option<String>,
}

#[derive(Debug)]
pub enum ValidationResult {
    Listing(Value),
    FlashToken { token: String, erase: bool },
}

pub struct Validator { client: Client }
impl Validator {
    pub fn new() -> Result<Self> { Ok(Self { client: Client::builder().user_agent("MiTunes_UserAgent_v3.0").build()? }) }
    pub fn validate(&self, info: &crate::device::DeviceInfo, md5: &str, flash: bool) -> Result<ValidationResult> {
        let key = [0x6D,0x69,0x75,0x69,0x6F,0x74,0x61,0x76,0x61,0x6C,0x69,0x64,0x65,0x64,0x31,0x31];
        let iv  = [0x30,0x31,0x30,0x32,0x30,0x33,0x30,0x34,0x30,0x35,0x30,0x36,0x30,0x37,0x30,0x38];
        let payload = RequestPayload { d:&info.device, v:&info.version, c:&info.codebase, b:&info.branch, sn:&info.sn, l:"en-US", f:"1", options: Options { zone: info.romzone.clone() }, pkg: md5 };
        let json = serde_json::to_vec(&payload).map_err(|e| Error::Other(e.to_string()))?;
        let enc = aes128_cbc_encrypt(&key, &iv, &pkcs7_pad(json))?;
        let b64 = general_purpose::STANDARD.encode(enc);
        let form = format!("q={}&t=&s=1", urlencoding::encode(&b64));
        let resp = self.client.post("http://update.miui.com/updates/miotaV3.php").body(form).send()?.bytes()?;
        // decrypt path
        let decoded = general_purpose::STANDARD.decode(&resp).map_err(|e| Error::Crypto(e.to_string()))?;
        let plain = aes128_cbc_decrypt(&key, &iv, &decoded)?;
        // extract json substring
        let text = String::from_utf8_lossy(&plain);
        if let (Some(s), Some(e)) = (text.find('{'), text.rfind('}')) {
            let slice = &text[s..=e];
            let v: Value = serde_json::from_str(slice).map_err(|e| Error::InvalidResponse(e.to_string()))?;
            if flash {
                if let Some(pkg_rom) = v.get("PkgRom") {
                    let token = pkg_rom.get("Validate").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let erase = pkg_rom.get("Erase").and_then(|x| x.as_str()).unwrap_or("0") == "1";
                    return Ok(ValidationResult::FlashToken { token, erase });
                }
                return Err(Error::InvalidResponse("PkgRom missing".into()));
            }
            Ok(ValidationResult::Listing(v))
        } else {
            Err(Error::InvalidResponse("JSON not found".into()))
        }
    }
}

fn aes128_cbc_decrypt(key: &[u8;16], iv: &[u8;16], cipher: &[u8]) -> Result<Vec<u8>> {
    #[cfg(feature = "openssl")] {
        use openssl::symm::{Cipher, Crypter, Mode};
        let cipher_type = Cipher::aes_128_cbc();
        let mut c = Crypter::new(cipher_type, Mode::Decrypt, key, Some(iv)).map_err(|e| Error::Crypto(e.to_string()))?;
        let mut out = vec![0u8; cipher.len() + cipher_type.block_size()];
        let mut count = c.update(cipher, &mut out).map_err(|e| Error::Crypto(e.to_string()))?;
        count += c.finalize(&mut out[count..]).map_err(|e| Error::Crypto(e.to_string()))?;
        out.truncate(count); return Ok(out);
    }
    #[cfg(not(feature = "openssl"))]
    {
        use aes::Aes128; use cbc::cipher::{KeyIvInit, BlockDecryptMut, block_padding::Pkcs7};
        type Aes128CbcDec = cbc::Decryptor<Aes128>;
        let dec = Aes128CbcDec::new_from_slices(key, iv).map_err(|e| Error::Crypto(e.to_string()))?;
        let out = dec.decrypt_padded_vec_mut::<Pkcs7>(cipher).map_err(|e| Error::Crypto(e.to_string()))?;
        Ok(out)
    }
}
