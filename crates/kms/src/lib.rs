//! KMS 抽象：加密 / 解密 owner EOA 私钥与 L2 secret。
//!
//! 对应 `docs/CHANNEL_A_SIGNING.md` §3.2 / §7「AWS KMS 接入（dev 路径用 env 明文）」。
//!
//! 三条路径：
//! - **`LocalKms`**（生产 · 自托管）：落盘 master key + AES-256-GCM 对称加密。master key 文件由
//!   env `SHARPSIDE_KMS_MASTER_KEY_PATH` 指定，不存在则自动生成（0600）。私钥落盘加密，不依赖云。
//! - **`DevKms`**：dev/测试用。明文透传（base64 包装），无任何加密——**仅 dev**，
//!   生产绝不可用。由 env `SHARPSIDE_KMS_DEV_PLAINTEXT=1` 显式开启，否则拒绝（防误用）。
//! - **`AwsKms`**（feature `aws`，stub）：云上生产路径。调 AWS KMS `Encrypt`/`Decrypt`，
//!   per-user KMS key。需 AWS 凭证与网络（待接 `aws-sdk-kms`）。
//! - **`FnKms`**：闭包注入，单测用。
//!
//! account 服务用 `encrypt` 写入 `user_venue_credentials.encrypted_blob`；
//! copier 服务用 `decrypt` 还原 owner EOA 私钥 / L2 secret 后签名。

#![forbid(unsafe_code)]

use thiserror::Error;

/// KMS 错误。
#[derive(Debug, Error)]
pub enum KmsError {
    #[error("KMS 操作失败: {0}")]
    Ops(String),
    #[error(
        "KMS 未启用：dev 路径需设 SHARPSIDE_KMS_DEV_PLAINTEXT=1；生产路径需 feature aws + AWS 凭证"
    )]
    NotEnabled,
    #[error("KMS 密文损坏或非本 KMS 产出: {0}")]
    Decode(String),
}

/// KMS 抽象。`encrypt` 产密文入库，`decrypt` 还原明文签名。
pub trait Kms: Send + Sync {
    fn encrypt(&self, plaintext: &str) -> Result<String, KmsError>;
    fn decrypt(&self, ciphertext: &str) -> Result<String, KmsError>;
    /// 标识，便于日志。
    fn name(&self) -> &'static str;
}

// ── DevKms ──

/// dev/测试 KMS：明文透传，可选 base64 包装。
///
/// **仅 dev**。`SHARPSIDE_KMS_DEV_PLAINTEXT=1` 时可用，否则所有操作返回 [`KmsError::NotEnabled`]。
/// 包装格式：base64(plaintext)，前缀 `dev:` 便于与真 KMS 密文区分。
pub struct DevKms {
    enabled: bool,
}

impl DevKms {
    /// 从 env 构造。`SHARPSIDE_KMS_DEV_PLAINTEXT=1` 启用。
    pub fn from_env() -> Self {
        let enabled = std::env::var("SHARPSIDE_KMS_DEV_PLAINTEXT").ok().as_deref() == Some("1");
        Self { enabled }
    }

    /// 强制启用（单测用）。
    pub fn enabled_for_test() -> Self {
        Self { enabled: true }
    }

    const PREFIX: &'static str = "dev:";
}

impl Kms for DevKms {
    fn name(&self) -> &'static str {
        "DevKms"
    }

    fn encrypt(&self, plaintext: &str) -> Result<String, KmsError> {
        if !self.enabled {
            return Err(KmsError::NotEnabled);
        }
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(plaintext.as_bytes());
        Ok(format!("{}{}", Self::PREFIX, b64))
    }

    fn decrypt(&self, ciphertext: &str) -> Result<String, KmsError> {
        if !self.enabled {
            return Err(KmsError::NotEnabled);
        }
        let b64 = ciphertext
            .strip_prefix(Self::PREFIX)
            .ok_or_else(|| KmsError::Decode("非 dev: 前缀".into()))?;
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| KmsError::Decode(format!("base64 解码失败: {e}")))?;
        String::from_utf8(bytes).map_err(|e| KmsError::Decode(format!("非 UTF-8: {e}")))
    }
}

// ── FnKms（单测注入）──

impl<F> Kms for F
where
    F: Fn(&str) -> Result<String, KmsError> + Send + Sync,
{
    fn name(&self) -> &'static str {
        "FnKms"
    }
    fn encrypt(&self, plaintext: &str) -> Result<String, KmsError> {
        self(plaintext)
    }
    fn decrypt(&self, ciphertext: &str) -> Result<String, KmsError> {
        self(ciphertext)
    }
}

// ── LocalKms（生产路径 · 落盘 master key + AES-256-GCM）──
//
// 不依赖云 KMS：用磁盘上一个 32 字节 master key 对 owner EOA 私钥 / L2 secret 做
// AES-256-GCM 对称加密。master key 文件由 env `SHARPSIDE_KMS_MASTER_KEY_PATH` 指定；
// 不存在则自动生成（0600 权限）。密文格式 `local:` + base64(nonce[12] || ct+tag)。
//
// 适用：单机 / 自托管部署（私钥落盘加密）。多实例须共享同一 master key 文件（或挂载 secret）。
// 云上可换 AwsKms（见下方 stub，待接 aws-sdk-kms）。
//
// 安全说明：master key 是整个系统的根密钥——泄露即所有密文可解。生产应：
//   1. master key 文件权限 0600，仅服务进程可读；
//   2. 备份 master key（丢失即所有用户私钥不可恢复）；
//   3. 不同环境（prod/staging）用不同 master key。

/// 落盘 master key 的 AES-256-GCM KMS。生产路径（自托管）。
pub struct LocalKms {
    master_key: zeroize::Zeroizing<[u8; 32]>,
}

impl LocalKms {
    const PREFIX: &'static str = "local:";
    const KEY_LEN: usize = 32;
    const NONCE_LEN: usize = 12;

    /// 从 env `SHARPSIDE_KMS_MASTER_KEY_PATH` 指定的文件读 master key。
    /// 文件不存在则生成随机 32 字节 key 并写入（0600 权限）。
    pub fn from_env() -> Result<Self, KmsError> {
        let path =
            std::env::var("SHARPSIDE_KMS_MASTER_KEY_PATH").map_err(|_| KmsError::NotEnabled)?;
        Self::from_path(&path)
    }

    /// 从指定路径读/建 master key。
    pub fn from_path(path: &str) -> Result<Self, KmsError> {
        let key = match std::fs::read(path) {
            Ok(bytes) => {
                if bytes.len() != Self::KEY_LEN {
                    return Err(KmsError::Ops(format!(
                        "master key 文件长度 {} != {}，请删除后重新生成",
                        bytes.len(),
                        Self::KEY_LEN
                    )));
                }
                let mut k = [0u8; 32];
                k.copy_from_slice(&bytes);
                k
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // 自动生成
                let mut k = [0u8; 32];
                use rand::RngCore;
                rand::thread_rng().fill_bytes(&mut k);
                Self::write_key(path, &k)?;
                tracing::info!(path, "LocalKms 已生成新 master key（0600）");
                k
            }
            Err(e) => return Err(KmsError::Ops(format!("读 master key 失败: {e}"))),
        };
        Ok(Self {
            master_key: zeroize::Zeroizing::new(key),
        })
    }

    #[cfg(unix)]
    fn write_key(path: &str, key: &[u8; 32]) -> Result<(), KmsError> {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .and_then(|mut f| std::io::Write::write_all(&mut f, key))
            .map_err(|e| KmsError::Ops(format!("写 master key 失败: {e}")))
    }

    #[cfg(not(unix))]
    fn write_key(path: &str, key: &[u8; 32]) -> Result<(), KmsError> {
        std::fs::write(path, key).map_err(|e| KmsError::Ops(format!("写 master key 失败: {e}")))
    }

    fn cipher(&self) -> aes_gcm::Aes256Gcm {
        use aes_gcm::KeyInit;
        aes_gcm::Aes256Gcm::new_from_slice(&self.master_key[..]).expect("32 字节 key 必然有效")
    }
}

impl Kms for LocalKms {
    fn name(&self) -> &'static str {
        "LocalKms"
    }

    fn encrypt(&self, plaintext: &str) -> Result<String, KmsError> {
        use aes_gcm::aead::Aead;
        use aes_gcm::Nonce;
        let cipher = self.cipher();
        let mut nonce_bytes = [0u8; Self::NONCE_LEN];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        #[allow(deprecated)] // aes-gcm 0.10 用 generic-array 0.14 的 from_slice
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ct = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| KmsError::Ops(format!("AES-GCM 加密失败: {e}")))?;
        // nonce || ct+tag
        let mut blob = Vec::with_capacity(Self::NONCE_LEN + ct.len());
        blob.extend_from_slice(&nonce_bytes);
        blob.extend_from_slice(&ct);
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&blob);
        Ok(format!("{}{}", Self::PREFIX, b64))
    }

    fn decrypt(&self, ciphertext: &str) -> Result<String, KmsError> {
        let b64 = ciphertext
            .strip_prefix(Self::PREFIX)
            .ok_or_else(|| KmsError::Decode("非 local: 前缀（非 LocalKms 密文）".into()))?;
        use base64::Engine;
        let blob = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| KmsError::Decode(format!("base64 解码失败: {e}")))?;
        if blob.len() < Self::NONCE_LEN + 16 {
            return Err(KmsError::Decode("密文过短".into()));
        }
        let (nonce_bytes, ct) = blob.split_at(Self::NONCE_LEN);
        use aes_gcm::aead::Aead;
        let cipher = self.cipher();
        #[allow(deprecated)] // aes-gcm 0.10 用 generic-array 0.14 的 from_slice
        let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);
        let pt = cipher.decrypt(nonce, ct).map_err(|e| {
            KmsError::Decode(format!("AES-GCM 解密失败（密钥不匹配或密文损坏）: {e}"))
        })?;
        String::from_utf8(pt).map_err(|e| KmsError::Decode(format!("非 UTF-8: {e}")))
    }
}

// ── AwsKms（云路径 stub，待接 aws-sdk-kms）──
//
// 真实 AWS KMS 实现需 `aws-sdk-kms` + AWS 凭证 + 网络（见 `docs/CHANNEL_A_SIGNING.md` §7）。
// 本机受限网络无法拉取 aws-sdk-kms，故此处仅提供 stub：所有操作返回 [`KmsError::NotEnabled`]。
// 自托管生产路径用 [`LocalKms`]（落盘 master key + AES-256-GCM）；云上替换此 stub 为
// `aws-sdk-kms::Client` 调用即可（接口与 LocalKms 一致）。
//
// 生产接入步骤：
// 1. 在 Cargo.toml 加 `aws-config` / `aws-sdk-kms`（feature-gated）；
// 2. 用 `AwsKms::from_env().await` 构造（load_from_env → Client）；
// 3. `encrypt` 调 `client.encrypt().key_id(...).plaintext(...).send()`，base64 编码 CiphertextBlob；
// 4. `decrypt` 调 `client.decrypt().ciphertext_blob(...).send()`，UTF-8 还原明文；
// 5. per-user KMS key_id 存 user 表 `kms_key_id` 列（未来迁移 0013）。

/// AWS KMS stub。云上生产路径替换为真实现（见模块注释）；自托管用 [`LocalKms`]。
pub struct AwsKms;

impl AwsKms {
    /// 占位构造。真实实现须 `async` load AWS config。
    pub fn from_env_stub() -> Self {
        Self
    }
}

impl Kms for AwsKms {
    fn name(&self) -> &'static str {
        "AwsKms(stub)"
    }
    fn encrypt(&self, _plaintext: &str) -> Result<String, KmsError> {
        Err(KmsError::NotEnabled)
    }
    fn decrypt(&self, _ciphertext: &str) -> Result<String, KmsError> {
        Err(KmsError::NotEnabled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_kms_disabled_by_default() {
        std::env::remove_var("SHARPSIDE_KMS_DEV_PLAINTEXT");
        let kms = DevKms::from_env();
        assert!(matches!(kms.encrypt("x"), Err(KmsError::NotEnabled)));
        assert!(matches!(kms.decrypt("y"), Err(KmsError::NotEnabled)));
    }

    #[test]
    fn dev_kms_round_trip() {
        let kms = DevKms::enabled_for_test();
        let ct = kms.encrypt("secret-private-key-hex").unwrap();
        assert!(ct.starts_with("dev:"));
        let pt = kms.decrypt(&ct).unwrap();
        assert_eq!(pt, "secret-private-key-hex");
    }

    #[test]
    fn dev_kms_rejects_foreign_ciphertext() {
        let kms = DevKms::enabled_for_test();
        // 非 dev: 前缀 → 拒
        assert!(kms.decrypt("AQICAHh...").is_err());
        // 损坏 base64
        assert!(kms.decrypt("dev:!!!not-base64!!!").is_err());
    }

    #[test]
    fn fn_kms_injection() {
        let kms = |s: &str| Ok(format!("echo:{s}"));
        assert_eq!(kms.encrypt("hi").unwrap(), "echo:hi");
    }

    #[test]
    fn local_kms_round_trip() {
        let path = tmp_key_path();
        let kms = LocalKms::from_path(&path).unwrap();
        let ct = kms
            .encrypt("0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80")
            .unwrap();
        assert!(ct.starts_with("local:"));
        let pt = kms.decrypt(&ct).unwrap();
        assert_eq!(
            pt,
            "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn local_kms_reuses_existing_key() {
        let path = tmp_key_path();
        let kms1 = LocalKms::from_path(&path).unwrap();
        let ct1 = kms1.encrypt("secret").unwrap();
        let kms2 = LocalKms::from_path(&path).unwrap();
        let pt = kms2.decrypt(&ct1).unwrap();
        assert_eq!(pt, "secret");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn local_kms_rejects_dev_ciphertext() {
        let path = tmp_key_path();
        let kms = LocalKms::from_path(&path).unwrap();
        assert!(kms.decrypt("dev:YWJjZA==").is_err());
        assert!(kms.decrypt("local:!!!not-base64!!!").is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn local_kms_wrong_key_cannot_decrypt() {
        let p1 = tmp_key_path();
        let p2 = tmp_key_path();
        let kms1 = LocalKms::from_path(&p1).unwrap();
        let kms2 = LocalKms::from_path(&p2).unwrap();
        let ct = kms1.encrypt("secret").unwrap();
        assert!(kms2.decrypt(&ct).is_err());
        let _ = std::fs::remove_file(&p1);
        let _ = std::fs::remove_file(&p2);
    }

    #[test]
    fn local_kms_from_env_requires_path() {
        std::env::remove_var("SHARPSIDE_KMS_MASTER_KEY_PATH");
        assert!(matches!(LocalKms::from_env(), Err(KmsError::NotEnabled)));
    }

    fn tmp_key_path() -> String {
        use rand::RngCore;
        let mut buf = [0u8; 8];
        rand::thread_rng().fill_bytes(&mut buf);
        let name = hex::encode(buf);
        std::env::temp_dir()
            .join(format!("sharpside-kms-test-{name}.key"))
            .to_string_lossy()
            .into_owned()
    }
}
