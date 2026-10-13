#!/bin/sh
# GizAi's one-line install for Linux and macOS:
#   curl -fsSL https://raw.githubusercontent.com/Yotech-AI/gizai/production/scripts/get.sh | sh
# It gets the source of the latest release, the tag Gizai's own update fetches (docs/RELEASING.md), into
# ${XDG_DATA_HOME:-~/.local/share}/gizai-src, or updates it there. Then it runs that release's install.sh with bash:
# install.sh --check first, then install.sh, which builds and installs Gizai as it does from a clone. Run it again to
# update. Nothing goes into Gizai's data folder. The source folder is this script's own: changes in it are replaced.
#
# Options go to install.sh, after sh -s --:   curl -fsSL <url> | sh -s -- --check   (see usage below)
# Environment: GIZAI_PREFIX and XDG_DATA_HOME go to install.sh too (curl -fsSL <url> | GIZAI_PREFIX=/opt/gizai sh).
# GIZAI_REPO is the git repository, GIZAI_RELEASES_URL what it asks for the latest release (by default GitHub's; also
# http and file URLs), and GIZAI_REF a tag, branch or commit to get instead of the latest release (for tests and CI).
# A POSIX sh script, for dash and macOS's sh. It does everything in main, called on its last line, so a download cut
# short runs nothing.

DEFAULT_REPO=https://github.com/Yotech-AI/gizai.git
GET_PS1=https://raw.githubusercontent.com/Yotech-AI/gizai/production/scripts/get.ps1

say() { printf '%s\n' "$*"; }
step() { printf '\n== %s\n' "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }

# "git", "git and bash", "git, bash and curl"
words() {
  case $# in
    1) printf '%s' "$1" ;;
    2) printf '%s and %s' "$1" "$2" ;;
    *) printf '%s, %s and %s' "$1" "$2" "$3" ;;
  esac
}

usage() {
  cat <<'USAGE'
GizAi's one-line install for Linux and macOS:
  curl -fsSL https://raw.githubusercontent.com/Yotech-AI/gizai/production/scripts/get.sh | sh
It gets the source of GizAi's latest release into ~/.local/share/gizai-src (or updates it there), then runs that
release's install.sh: --check first, then the install. Run it again to update.

Options go to install.sh, after sh -s --:
  curl -fsSL <url> | sh -s -- --check       get the source and show what is missing
  curl -fsSL <url> | sh -s -- --uninstall   remove GizAi; add --purge to delete your data and the source
  --build-only and --skip-build work as they do for install.sh (see install.sh --help).
Environment: GIZAI_PREFIX (default ~/.local) and XDG_DATA_HOME, as for install.sh; GIZAI_REPO, GIZAI_RELEASES_URL,
and GIZAI_REF (a tag, branch or commit to get instead of the latest release, for tests).
USAGE
}

# git in the source folder's own repository only, never in one around it (a home folder kept in git, say).
src_git() { git -C "$dir" --git-dir="$dir/.git" --work-tree="$dir" "$@"; }

main() {
  unset CDPATH GIT_DIR GIT_WORK_TREE
  check=0 uninstall=0 tag=""
  for arg in "$@"; do
    case "$arg" in
      --check) check=1 ;;
      --uninstall) uninstall=1 ;;
      -h|--help) usage; return 0 ;;
    esac
  done

  os="$(uname -s)"
  case "$os" in
    Linux|Darwin) ;;
    *) say "This installs GizAi on Linux and macOS. On Windows, run this in PowerShell: irm $GET_PS1 | iex"; return 1 ;;
  esac

  # What this script needs itself: git and bash, and curl to ask for the latest release. On a Mac, /usr/bin/git only
  # offers to install Apple's Command Line Tools until they are there.
  missing=""
  if ! have git; then
    missing="git"
  elif [ "$os" = Darwin ] && [ "$(command -v git)" = /usr/bin/git ] && ! xcode-select -p >/dev/null 2>&1; then
    missing="git"
  fi
  have bash || missing="$missing bash"
  [ -n "${GIZAI_REF:-}" ] || have curl || missing="$missing curl"
  if [ "$os" = Darwin ] && [ "$missing" = git ]; then
    say "To install GizAi, first install Apple's Command Line Tools, which bring git: run xcode-select --install, then run this again."
    return 1
  elif [ -n "$missing" ]; then
    # shellcheck disable=SC2086 # one word each
    say "To install GizAi, first install $(words $missing) with your package manager, then run this again."
    return 1
  fi

  repo="${GIZAI_REPO:-$DEFAULT_REPO}"
  # a folder of the repository: as a full path, as git runs in the source folder
  if [ -d "$repo" ]; then repo="$(cd "$repo" && pwd -P)"; fi
  ref="${GIZAI_REF:-}"
  if [ -z "$ref" ]; then
    url="${GIZAI_RELEASES_URL:-}"
    if [ -z "$url" ]; then
      case "$repo" in
        https://github.com/*|git@github.com:*|ssh://git@github.com/*)
          name="${repo#*github.com[/:]}"; name="${name%/}"; name="${name%.git}"
          url="https://api.github.com/repos/$name/releases/latest" ;;
        *)
          say "$repo isn't on GitHub, so there is no latest release to ask it for: set GIZAI_RELEASES_URL or GIZAI_REF."
          return 1 ;;
      esac
    fi
    step "Finding GizAi's latest release"
    # As Gizai's own release check: -q first, so your ~/.curlrc plays no part, and redirects to https only.
    answer="$(curl -q --fail --silent --show-error --location --proto '=https,http,file' --proto-redir '=https' \
      --max-time 60 --header 'Accept: application/vnd.github+json' --header 'X-GitHub-Api-Version: 2022-11-28' \
      "$url")" || {
      say "Could not read the latest release from $url. Check your internet connection, then run this again."
      return 1
    }
    tag="$(printf '%s\n' "$answer" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
    if ! printf '%s\n' "$tag" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$'; then
      say "The latest release's tag, ${tag:-missing}, isn't a version like v1.2.3, so there is nothing to install."
      return 1
    fi
    say "Latest release: $tag"
  fi
  what="${tag:-$ref}"

  dir="${XDG_DATA_HOME:-$HOME/.local/share}/gizai-src"
  step "Getting $what into $dir"
  mkdir -p "$dir" && dir="$(cd "$dir" && pwd -P)" || { say "Could not make $dir."; return 1; }
  # A .git that git can't use (one cut short) is made again.
  if ! src_git rev-parse --git-dir >/dev/null 2>&1; then
    rm -rf "$dir/.git"
    git -c init.defaultBranch=main init --quiet "$dir" || { say "Could not make a git repository in $dir."; return 1; }
  fi
  # As Gizai's own update: a shallow fetch of the tag alone, checked out as it is, and everything else in the folder
  # goes, except the build's target/, so the next build is quicker.
  if [ -n "$ref" ]; then spec="$ref"; else spec="+refs/tags/$tag:refs/tags/$tag"; fi
  src_git fetch --depth 1 --no-tags --force -- "$repo" "$spec" || { say "Could not get $what from $repo."; return 1; }
  src_git checkout --quiet --force --detach FETCH_HEAD && src_git clean -ffdxq --exclude=/target/ || {
    say "Could not put $what in place in $dir."
    return 1
  }
  if [ ! -f "$dir/install.sh" ]; then
    say "$what has no install.sh, so it can't be installed this way."
    return 1
  fi

  # The release's check first, which says what is missing. Not before --uninstall: install.sh --check --uninstall
  # would uninstall.
  if [ "$uninstall" = 0 ]; then
    bash "$dir/install.sh" --check "$@" </dev/null
    status=$?
    if [ "$status" != 0 ]; then
      if [ "$status" = 1 ] && [ "$check" = 0 ]; then
        say ""
        say "Install what is missing above, then run this again."
      fi
      return "$status"
    fi
    if [ "$check" = 1 ]; then return 0; fi
  fi
  bash "$dir/install.sh" "$@" </dev/null
}

main "$@"
