# Releasing Gizai

Gizai's repository has two branches:

| Branch | What it holds | How it changes |
|---|---|---|
| `main` | Development. Cards' branches start here, and their pull requests merge here. | Pull requests, merged on GitHub |
| `production` | The released version: what `install.sh` installs and what Gizai updates to. | Only a pull request from `main`. It is protected: no direct pushes, no force pushes, no deleting. |

A release is a GitHub Release with a version tag (`v0.2.0`) on `production`. Gizai's update check, planned in the "Update from the sidebar" card, looks only at Releases, never at `main`.

## Making a release

1. **Bump the version** on `main`, in all three places: `Cargo.toml` (the workspace version), `package.json` and `src-tauri/tauri.conf.json`.
2. **Open a pull request** from `main` into `production` on GitHub, check it, and merge it:

   ```sh
   gh pr create --base production --head main --title "Release v0.2.0"
   ```

3. **Create the release** from `production`, with notes made from the merged pull requests:

   ```sh
   gh release create v0.2.0 --target production --generate-notes
   ```

People who installed from `production` update with `./install.sh`, and later with the Update item in Gizai's sidebar.
