#!/bin/bash
# Make MailCraft's TLS trust store work on aarch64-linux-android.
#
# Why this exists: `mailcraft-net` checks every server certificate with
# `rustls-platform-verifier`, whose Android backend is a JVM component distributed as an AAR
# (`org.rustls:rustls-platform-verifier-android`) from a Maven repository. That component is not in
# this Gradle shell, and `Verifier::new` fails at runtime without it ("the system's certificate
# check couldn't be set up"). The ArtCraft Launcher hit exactly this and solved it the same way: on
# Android read the system's own root store, which is what Android's TrustManager consults.
#
# `/system/etc/security/cacerts` holds one PEM file per root (measured: 149 files on the
# PA2573). The chain, the dates, the purpose and the server's name are still checked by
# `WebPkiServerVerifier`; what is given up is user-installed roots and revocation checking.
#
# Desktop builds are untouched: the hook picks `Verifier::new` there, as upstream does.
#
# Applies to crates/net/src/tls.rs. Idempotent; run it before every build (build-so.sh does).
#   setup-tls-android.sh [clone-dir]
set -euo pipefail

CLONE="${1:-$HOME/artcraft-android/src/mailcraft}"
TLS="$CLONE/crates/net/src/tls.rs"
[ -f "$TLS" ] || { echo "FAIL: no $TLS" >&2; exit 2; }

/usr/bin/python3 - "$TLS" <<'PY'
import pathlib, sys

path = pathlib.Path(sys.argv[1])
text = path.read_text()

if "fn platform_verifier(" in text:
    print("TLS patch already applied")
    raise SystemExit(0)

# The two places the system's verifier is built, and the one static that holds one.
ORIGINAL_CLIENT = """    let verifier = Verifier::new(provider.clone()).map_err(|e| format!("the system's certificate check couldn't be set up ({e})"))?;
    let config = build(provider, Arc::new(verifier), purpose)?;"""
PATCHED_CLIENT = """    let verifier = platform_verifier(provider.clone())?;
    let config = build(provider, verifier, purpose)?;"""

ORIGINAL_SYSTEM = """    static SYSTEM: OnceLock<Arc<Verifier>> = OnceLock::new();
    if let Some(verifier) = SYSTEM.get() {
        return Ok(verifier.clone());
    }
    let verifier = Verifier::new(provider.clone()).map_err(|e| format!("the system's certificate check couldn't be set up ({e})"))?;
    Ok(SYSTEM.get_or_init(|| Arc::new(verifier)).clone())"""
PATCHED_SYSTEM = """    static SYSTEM: OnceLock<Arc<dyn ServerCertVerifier>> = OnceLock::new();
    if let Some(verifier) = SYSTEM.get() {
        return Ok(verifier.clone());
    }
    let verifier = platform_verifier(provider.clone())?;
    Ok(SYSTEM.get_or_init(|| verifier).clone())"""

ORIGINAL_IMPORT = "use rustls_platform_verifier::Verifier;"
PATCHED_IMPORT = """#[cfg(any(test, not(target_os = "android")))]
use rustls_platform_verifier::Verifier;"""

ANCHOR = """    Ok(SYSTEM.get_or_init(|| verifier).clone())
}
"""

HELPERS = '''    Ok(SYSTEM.get_or_init(|| verifier).clone())
}

/// The system's certificate check: the operating system's own verifier on the desktop, and on
/// Android the system's root store read directly.
///
/// Android-port: `rustls-platform-verifier`'s Android backend is a JVM component distributed as an
/// AAR from a Maven repository, and this build's Gradle shell has no dependency on it, so
/// `Verifier::new` would fail at runtime with a missing class. The system's store is still what
/// decides: `/system/etc/security/cacerts` holds the same roots Android's own TrustManager
/// consults, one PEM file per root. What this gives up is user-installed roots and revocation
/// checking -- the same trade the ArtCraft Launcher's Android port made.
#[cfg(not(target_os = "android"))]
fn platform_verifier(provider: Arc<CryptoProvider>) -> Result<Arc<dyn ServerCertVerifier>, String> {
    let verifier = Verifier::new(provider).map_err(|e| format!("the system's certificate check couldn't be set up ({e})"))?;
    Ok(Arc::new(verifier))
}

/// Android: see the `cfg(not(target_os = "android"))` twin above.
#[cfg(target_os = "android")]
fn platform_verifier(provider: Arc<CryptoProvider>) -> Result<Arc<dyn ServerCertVerifier>, String> {
    use rustls::RootCertStore;
    use rustls::pki_types::CertificateDer;
    use rustls::pki_types::pem::PemObject;

    /// Android's system trust store: one PEM file per root, named by subject hash.
    const DIR: &str = "/system/etc/security/cacerts";

    // Read once: every connection shares the answer, and the store does not change under a running
    // app. A store that couldn't be read is remembered too, so the reason is logged once.
    static ROOTS: OnceLock<Result<Arc<RootCertStore>, String>> = OnceLock::new();
    let roots = ROOTS
        .get_or_init(|| {
            let entries = std::fs::read_dir(DIR).map_err(|e| format!("no system certificate store at {DIR} ({e})"))?;
            let mut store = RootCertStore::empty();
            let mut read = 0usize;
            for entry in entries.flatten() {
                match CertificateDer::from_pem_file(entry.path()) {
                    Ok(cert) => match store.add(cert) {
                        Ok(()) => read += 1,
                        Err(e) => log::debug!("{}: {e}", entry.path().display()),
                    },
                    Err(e) => log::debug!("{}: {e}", entry.path().display()),
                }
            }
            log::info!("{read} system root certificates read from {DIR}");
            if read == 0 {
                return Err(format!("no certificates could be read from {DIR}"));
            }
            Ok(Arc::new(store))
        })
        .clone()?;
    let verifier = rustls::client::WebPkiServerVerifier::builder_with_provider(roots, provider)
        .build()
        .map_err(|e| format!("the system's certificate check couldn't be set up ({e})"))?;
    let verifier: Arc<dyn ServerCertVerifier> = verifier;
    Ok(verifier)
}
'''

for old in (ORIGINAL_CLIENT, ORIGINAL_SYSTEM, ORIGINAL_IMPORT):
    if old not in text:
        print(f"FAIL: the anchor text is not in {path} -- upstream changed:\n{old}", file=sys.stderr)
        raise SystemExit(2)

text = text.replace(ORIGINAL_CLIENT, PATCHED_CLIENT, 1)
text = text.replace(ORIGINAL_SYSTEM, PATCHED_SYSTEM, 1)
text = text.replace(ORIGINAL_IMPORT, PATCHED_IMPORT, 1)
if ANCHOR not in text:
    print("FAIL: the helper's insertion point is not in the file", file=sys.stderr)
    raise SystemExit(2)
text = text.replace(ANCHOR, HELPERS, 1)

path.write_text(text)
print("TLS patch applied to", path)
PY
