#!/usr/bin/env python3
"""Create a Finder-launchable app; credentials are read only from the environment."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile


def run_private(command):
    # Signing tools may include certificate identities in diagnostics. Do not
    # echo their command line or output into build logs in the checkout.
    result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        raise RuntimeError(f"{Path(command[0]).name} failed (exit {result.returncode}); "
                           "check your external signing/notarization configuration")
    return result.stdout


def sign(app):
    # CI runners have no certificate identity. An ad hoc signature requires
    # neither a certificate nor credentials and allows Apple Silicon to run it.
    identity = "-" if os.environ.get("CI") else (os.environ.get("PTE_MACOS_SIGN_IDENTITY") or "-")
    command = ["/usr/bin/codesign", "--force", "--sign", identity]
    if identity != "-":
        command += ["--options", "runtime", "--timestamp"]
    run_private(command + [str(app)])
    run_private(["/usr/bin/codesign", "--verify", "--strict", str(app)])


def notarize(app):
    if os.environ.get("CI"):
        print("CI app uses an ad hoc signature; certificate signing and notarization are local only.")
        return
    profile = os.environ.get("PTE_NOTARY_KEYCHAIN_PROFILE")
    key = os.environ.get("PTE_NOTARY_KEY_PATH")
    key_id = os.environ.get("PTE_NOTARY_KEY_ID")
    issuer = os.environ.get("PTE_NOTARY_ISSUER")
    if not any((profile, key, key_id, issuer)):
        print("No notarization configuration supplied; app is not notarized.")
        return
    if (os.environ.get("PTE_MACOS_SIGN_IDENTITY") or "-") == "-":
        raise RuntimeError("Notarization requires PTE_MACOS_SIGN_IDENTITY (Developer ID Application)")
    if profile:
        credentials = ["--keychain-profile", profile]
    elif key and key_id and issuer:
        key_path = Path(key).expanduser().resolve()
        repository = Path(__file__).resolve().parent.parent
        if key_path.is_relative_to(repository):
            raise RuntimeError("Keep notarization keys outside the repository")
        credentials = ["--key", str(key_path), "--key-id", key_id, "--issuer", issuer]
    else:
        raise RuntimeError("Supply a notary Keychain profile or all three API key variables")
    # Apple's upload archive is temporary and outside the repository. Never
    # copy private keys, certificates, profiles or identities into the app.
    with tempfile.TemporaryDirectory(prefix="pte-notary-") as temporary:
        archive = str(Path(temporary) / "PTE Explorer.zip")
        run_private(["/usr/bin/ditto", "-c", "-k", "--keepParent", str(app), archive])
        response = run_private(["/usr/bin/xcrun", "notarytool", "submit", archive,
                                *credentials, "--wait", "--output-format", "json"])
        if json.loads(response).get("status") != "Accepted":
            raise RuntimeError("Apple did not accept notarization; inspect the submission in your external account")
    run_private(["/usr/bin/xcrun", "stapler", "staple", str(app)])
    run_private(["/usr/bin/xcrun", "stapler", "validate", str(app)])
    print("Notarization accepted and ticket stapled.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path)
    parser.add_argument("--app", type=Path, required=True)
    parser.add_argument("--version", default="0.1.0")
    parser.add_argument("--notarize-if-configured", action="store_true")
    args = parser.parse_args()
    if sys.platform != "darwin":
        parser.error("Run macOS packaging on macOS")
    app = args.app.resolve()
    if args.executable:
        resources = Path(__file__).resolve().parent.parent / "packaging" / "macos"
        # Rebuild from clean contents so stale files or old signatures cannot
        # become part of the next app. Only the designated .app is removed.
        if app.suffix != ".app":
            parser.error("--app must name an .app bundle")
        if app.exists():
            shutil.rmtree(app)
        (app / "Contents" / "MacOS").mkdir(parents=True)
        (app / "Contents" / "Resources").mkdir()
        shutil.copy2(args.executable, app / "Contents" / "MacOS" / "pte-explorer")
        shutil.copy2(resources / "PteExplorer.icns", app / "Contents" / "Resources")
        with (resources / "Info.plist").open("rb") as source:
            info = plistlib.load(source)
        info["CFBundleVersion"] = info["CFBundleShortVersionString"] = args.version
        with (app / "Contents" / "Info.plist").open("wb") as output:
            plistlib.dump(info, output)
        (app / "Contents" / "PkgInfo").write_bytes(b"APPL????")
    elif not (app / "Contents" / "Info.plist").is_file():
        parser.error("Existing app bundle is missing")
    sign(app)
    if args.notarize_if_configured:
        notarize(app)
    print(f"App bundle ready: {app}")


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError) as error:
        sys.exit(str(error))
