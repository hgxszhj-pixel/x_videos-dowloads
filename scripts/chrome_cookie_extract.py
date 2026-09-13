#!/usr/bin/env python3
"""
chrome_cookie_extract.py - 从 Chrome 本地加密 Cookies SQLite 读 SHANHU/万字 cookie 并解密

背景:Chrome 80+ 用 AES-128-CBC + PKCS7 加密 cookie (v10 前缀),
   密钥派生: PBKDF2-HMAC-SHA1(Keychain_pw, "saltysalt", 1003, 16)。
   Chrome 在 macOS Keychain 注册的 service = "Chrome Safe Storage"。

   第一次跑会触发 macOS 弹窗 (security 访问 Keychain),
   选"总是允许"省事。

用法:
  python3 scripts/chrome_cookie_extract.py --output-cookie-string
  # → 输出 'PHPSESSID=...; tengface_user_login_uuid=...; ...' 到 stdout

  python3 scripts/chrome_cookie_extract.py \
    | xargs -I {} cargo run -- --shanhu-login-cookie "{}"

可选:
  --profile-dir <path>    Chrome profile 路径,默认 ~/Library/Application Support/Google/Chrome/Default
  --domains wanzibook.com rexxi.cn tenwiseacademy.cn   域名白名单 (LIKE 匹配)
  --include-expired       包含过期 cookie (默认跳过)
"""
import argparse
import hashlib
import shutil
import sqlite3
import subprocess
import sys
import tempfile
from pathlib import Path


KEYCHAIN_SERVICE = "Chrome Safe Storage"
PBKDF2_SALT = b"saltysalt"
PBKDF2_ITERATIONS = 1003
V10_PREFIX = b"v10"
DEFAULT_PROFILE = Path.home() / "Library/Application Support/Google/Chrome/Default"
DEFAULT_DOMAINS = ["wanzibook.com", "rexxi.cn", "tenwiseacademy.cn"]


def get_chrome_safe_storage_password() -> str:
    """从 macOS Keychain 读 Chrome Safe Storage 密码。

    第一次跑会弹 macOS 授权 (security 访问 Keychain)。
    返回的密码是 arbitrary string,会被 PBKDF2 二次派生。
    """
    try:
        out = subprocess.run(
            ["security", "find-generic-password", "-s", KEYCHAIN_SERVICE, "-w"],
            capture_output=True,
            text=True,
            check=True,
        )
    except subprocess.CalledProcessError as e:
        stderr = e.stderr.strip() or "<empty>"
        sys.exit(
            f"❌ Keychain '{KEYCHAIN_SERVICE}' 读取失败:\n{stderr}\n"
            f"提示: 第一次跑会弹 macOS 授权框,选'总是允许'\n"
            f"      或手动跑 `security find-generic-password -s '{KEYCHAIN_SERVICE}' -w` 触发弹窗"
        )
    pw = out.stdout.strip()
    if not pw:
        sys.exit(f"❌ Keychain '{KEYCHAIN_SERVICE}' 密码为空 (Chrome 可能从未登录过任何站点)")
    return pw


def derive_aes_key(password: str) -> bytes:
    """PBKDF2-HMAC-SHA1 → AES-128 16-byte key."""
    return hashlib.pbkdf2_hmac(
        "sha1", password.encode("utf-8"), PBKDF2_SALT, PBKDF2_ITERATIONS, dklen=16
    )


def decrypt_v10_cookie(encrypted: bytes, key: bytes) -> str | None:
    """解密 Chrome v10 格式 cookie:
       [3 byte "v10"] [16 byte IV] [ciphertext AES-128-CBC + PKCS7]
    """
    if len(encrypted) < 3 + 16 + 16 or encrypted[:3] != V10_PREFIX:
        return None
    try:
        from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
        from cryptography.hazmat.backends import default_backend
    except ImportError:
        sys.exit(
            "❌ 缺 cryptography 库,跑: pip install cryptography\n"
            "   (或 pip3 install --user cryptography)"
        )

    iv = encrypted[3:19]
    ciphertext = encrypted[19:]
    cipher = Cipher(algorithms.AES(key), modes.CBC(iv), backend=default_backend())
    decryptor = cipher.decryptor()
    padded = decryptor.update(ciphertext) + decryptor.finalize()
    # Strip PKCS7 padding
    pad_len = padded[-1]
    if pad_len < 1 or pad_len > 16 or padded[-pad_len:] != bytes([pad_len]) * pad_len:
        return None  # 损坏的 padding → 解密失败
    plain = padded[:-pad_len]
    try:
        return plain.decode("utf-8")
    except UnicodeDecodeError:
        return None


def chrome_epoch_to_unix(expires_utc: int) -> int:
    """Chrome 时间戳是 Windows epoch 微秒 (1601-01-01 起)。"""
    return expires_utc // 1_000_000 - 11_644_473_600


def extract_cookies(profile_dir: Path, domains: list[str], include_expired: bool):
    """读 profile_dir/Cookies SQLite,过滤 + 解密 SHANHU 域 cookie。"""
    cookies_db = profile_dir / "Cookies"
    if not cookies_db.exists():
        sys.exit(f"❌ Cookies 数据库不存在: {cookies_db}\n提示: Chrome 是否装在该路径?")

    # Chrome 运行时锁 SQLite → 复制到临时文件再读
    with tempfile.NamedTemporaryFile(suffix=".sqlite", delete=False) as tmp:
        tmp_path = Path(tmp.name)
    try:
        shutil.copy2(cookies_db, tmp_path)
        conn = sqlite3.connect(f"file:{tmp_path}?mode=ro", uri=True)
    finally:
        try:
            tmp_path.unlink()
        except FileNotFoundError:
            pass

    try:
        # 域 LIKE 匹配 — Chrome host_key 通常带前导 '.' (".wanzibook.com")
        where_clauses = " OR ".join(["host_key LIKE ?"] * len(domains))
        params = [f"%{d}%" for d in domains]
        cur = conn.execute(
            f"SELECT name, encrypted_value, host_key, expires_utc "
            f"FROM cookies WHERE {where_clauses}",
            params,
        )
        now_unix = int(__import__("time").time())
        results: list[tuple[str, str, str]] = []
        for name, encrypted, host, expires_utc in cur:
            if not encrypted:
                continue
            if expires_utc > 0 and not include_expired:
                if chrome_epoch_to_unix(expires_utc) < now_unix:
                    continue
            value = decrypt_v10_cookie(encrypted, key)
            if value:
                results.append((name, value, host))
        return results
    finally:
        conn.close()


def main():
    ap = argparse.ArgumentParser(
        description="从 Chrome 本地加密 cookie DB 读 SHANHU 域 cookie 并解密"
    )
    ap.add_argument(
        "--profile-dir",
        type=Path,
        default=DEFAULT_PROFILE,
        help=f"Chrome profile 目录 (默认: {DEFAULT_PROFILE})",
    )
    ap.add_argument(
        "--domains",
        nargs="+",
        default=DEFAULT_DOMAINS,
        help=f"域名白名单 (默认: {' '.join(DEFAULT_DOMAINS)})",
    )
    ap.add_argument(
        "--include-expired",
        action="store_true",
        help="包含过期 cookie",
    )
    ap.add_argument(
        "--output-cookie-string",
        action="store_true",
        help="输出 'name=value; name2=value2' 格式 (供 --shanhu-login-cookie 用)",
    )
    ap.add_argument(
        "--output-state-json",
        type=Path,
        default=None,
        help="输出 camoufox 风格 state.json 路径",
    )
    args = ap.parse_args()

    # 1. Keychain 密码 → AES key
    print(f"🔑 读 Keychain '{KEYCHAIN_SERVICE}' (会弹 macOS 授权框,选'总是允许')...", file=sys.stderr)
    password = get_chrome_safe_storage_password()
    global key  # 给 extract_cookies 用
    key = derive_aes_key(password)
    print(f"   ✓ PBKDF2 派生 key 完成 ({len(key)} bytes)", file=sys.stderr)

    # 2. 解密 SHANHU 域 cookie
    print(f"📂 读 {args.profile_dir}/Cookies ...", file=sys.stderr)
    entries = extract_cookies(args.profile_dir, args.domains, args.include_expired)

    if not entries:
        sys.exit(
            f"❌ 未找到匹配 cookie。检查:\n"
            f"   1. Chrome 是否登录过 wx-bgfcbieh.rexxi.cn 或 study.wanzibook.com\n"
            f"   2. --domains 参数是否包含正确域名 (当前: {args.domains})\n"
            f"   3. --profile-dir 是否指向正确的 Chrome profile"
        )

    print(f"   ✓ 找到 {len(entries)} 个有效 cookie", file=sys.stderr)
    for name, _, host in entries:
        print(f"     - {name} @ {host}", file=sys.stderr)

    # 3. 输出格式
    if args.output_cookie_string:
        cookie_str = "; ".join(f"{name}={value}" for name, value, _ in entries)
        print(cookie_str)  # stdout
    elif args.output_state_json:
        import json
        cookies = [
            {
                "name": name,
                "value": value,
                "domain": ".wanzibook.com",  # HttpOnly cookie 没法 inspect host,统一
                "path": "/",
                "expires": -1,
                "httpOnly": True,
                "secure": True,
                "sameSite": "Lax",
            }
            for name, value, _ in entries
        ]
        args.output_state_json.write_text(
            json.dumps({"cookies": cookies, "origins": []}, indent=2, ensure_ascii=False)
        )
        print(f"✓ 已写 {len(cookies)} cookies → {args.output_state_json}", file=sys.stderr)
    else:
        # 默认:表格输出
        print(f"\n{'NAME':<35} {'HOST':<25} VALUE")
        print("-" * 100)
        for name, value, host in entries:
            display_value = value[:40] + "..." if len(value) > 40 else value
            print(f"{name:<35} {host:<25} {display_value}")


if __name__ == "__main__":
    main()
