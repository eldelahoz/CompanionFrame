//! AlecaFrame's `lastData.dat`: AES-128-CBC with a fixed key/IV, wrapping the inventory JSON.
//! Port of `services/alecaFrameInventory.ts`.

use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use serde_json::Value;

use super::payload::{has_inventory_shape, unwrap_inventory_payload};

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

const KEY: [u8; 16] = [
    76, 69, 79, 45, 65, 76, 69, 67, 9, 69, 79, 45, 65, 76, 69, 67,
];
const IV: [u8; 16] = [
    49, 50, 70, 71, 66, 51, 54, 45, 76, 69, 51, 45, 113, 61, 57, 0,
];

/// The decrypted cache text, before JSON parsing rounds any 64-bit integers.
pub fn decrypt_text(data: &[u8]) -> Result<String, String> {
    let plain = Aes128CbcDec::new(&KEY.into(), &IV.into())
        .decrypt_padded_vec_mut::<Pkcs7>(data)
        .map_err(|e| format!("AlecaFrame data could not be decrypted: {e}"))?;
    String::from_utf8(plain).map_err(|e| e.to_string())
}

pub fn parse_buffer(data: &[u8]) -> Result<Value, String> {
    let parsed: Value = serde_json::from_str(&decrypt_text(data)?).map_err(|e| e.to_string())?;
    let unwrapped = unwrap_inventory_payload(parsed, Some(6));
    if !has_inventory_shape(&unwrapped) {
        return Err("AlecaFrame data did not contain a Warframe inventory payload".into());
    }
    Ok(unwrapped)
}

#[cfg(test)]
pub(crate) fn encrypt(payload: &Value) -> Vec<u8> {
    use aes::cipher::BlockEncryptMut;
    cbc::Encryptor::<aes::Aes128>::new(&KEY.into(), &IV.into())
        .encrypt_padded_vec_mut::<Pkcs7>(payload.to_string().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decrypts_payloads_with_nested_inventory_json() {
        let inv = json!({"Suits": [{"ItemType": "/Lotus/Powersuits/Excalibur/Excalibur"}], "RawUpgrades": []});
        let enc = encrypt(&json!({"InventoryJson": inv.to_string()}));
        let parsed = parse_buffer(&enc).unwrap();
        assert_eq!(
            parsed["Suits"][0]["ItemType"],
            "/Lotus/Powersuits/Excalibur/Excalibur"
        );
    }

    #[test]
    fn rejects_decrypted_data_without_inventory_arrays() {
        let err = parse_buffer(&encrypt(&json!({"hello": "world"}))).unwrap_err();
        assert!(err.contains("inventory payload"));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_buffer(b"not aes at all!!").is_err());
    }
}
