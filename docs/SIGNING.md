# Tauri Updater Signing

The app uses Tauri's updater signing (minisign) to cryptographically verify release artifacts before updates.

## Key Files

- Private key: `$HOME/.tauri/usage-indicator.key` (don't commit)
- Public key: `$HOME/.tauri/usage-indicator.key.pub`
- Public key is embedded in `src-tauri/tauri.conf.json`

## Backup the Private Key

If you lose the private key, you can't sign future updates. Back it up somewhere safe.

```bash
# Encrypt with GPG
gpg --symmetric --cipher-algo AES256 "$HOME/.tauri/usage-indicator.key"

# Store the .gpg file in your password manager or encrypted backup
```

To restore:
```bash
gpg -d usage-indicator.key.gpg > "$HOME/.tauri/usage-indicator.key"
```

## GitHub Secrets

These are already configured:

```bash
# Update if needed
cat "$HOME/.tauri/usage-indicator.key" | gh secret set TAURI_SIGNING_PRIVATE_KEY
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD  # prompts for password
```

## How It Works

GitHub Actions uses the private key to sign artifacts during release builds. The public key in `tauri.conf.json` verifies signatures before applying updates. Each artifact gets a `.sig` file and there's a `latest.json` manifest for the updater.

## Key Rotation

If you need to rotate the key (compromise, etc.), you'll need to do a manual upgrade cycle since old installations can't verify the new signature. Generate new key, update config and secrets, release as major version.
