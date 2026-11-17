#!/usr/bin/env bash
# Usage : package-manifests.sh vX.Y.Z DOSSIER [SHA256SUMS.txt]
# Génère, pour une release, la formule Homebrew (DOSSIER/Formula/rainhill.rb) et
# le manifeste Scoop (DOSSIER/bucket/rainhill.json). Les sommes SHA256 sont lues
# dans le fichier donné, sinon dans le SHA256SUMS.txt de la release publiée.
#
# Avec DOSSIER = racine du dépôt, le dépôt lui-même sert de tap Homebrew et de
# bucket Scoop : pas besoin de dépôts séparés.
set -euo pipefail

tag="${1:?tag manquant (ex. v1.0.0)}"
out="${2:?dossier de sortie manquant}"
sums_file="${3:-}"
repo="${GITHUB_REPOSITORY:-SmasBalloon/train}"

[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "Tag invalide : $tag" >&2; exit 1; }
version="${tag#v}"
base="https://github.com/$repo/releases/download/$tag"
desc="Jeu incrémental de réseau ferroviaire dans le terminal, de 1825 à demain"

if [ -n "$sums_file" ]; then
  sums=$(cat "$sums_file")
else
  sums=$(curl -fsSL "$base/SHA256SUMS.txt")
fi
sum_for() {
  local s
  s=$(printf '%s\n' "$sums" | awk -v f="$1" '$2 == f { print $1 }')
  [[ "$s" =~ ^[0-9a-f]{64}$ ]] || { echo "Somme introuvable pour $1" >&2; exit 1; }
  printf '%s' "$s"
}
asset() { printf 'rail-%s-%s' "$tag" "$1"; }

mac_arm=$(asset aarch64-apple-darwin);      mac_arm_sum=$(sum_for "$mac_arm.tar.gz")
mac_x86=$(asset x86_64-apple-darwin);       mac_x86_sum=$(sum_for "$mac_x86.tar.gz")
lin_arm=$(asset aarch64-unknown-linux-gnu); lin_arm_sum=$(sum_for "$lin_arm.tar.gz")
lin_x86=$(asset x86_64-unknown-linux-gnu);  lin_x86_sum=$(sum_for "$lin_x86.tar.gz")
win_x86=$(asset x86_64-pc-windows-msvc);    win_x86_sum=$(sum_for "$win_x86.zip")
win_arm=$(asset aarch64-pc-windows-msvc);   win_arm_sum=$(sum_for "$win_arm.zip")

mkdir -p "$out/Formula" "$out/bucket"

# ---------------------------------------------------------------- Homebrew
cat > "$out/Formula/rainhill.rb" <<EOF
# Généré par .github/scripts/package-manifests.sh ($repo) : ne pas modifier à la main.
class Rainhill < Formula
  desc "$desc"
  homepage "https://github.com/$repo"
  version "$version"
  # Licence maison : usage et modifications libres, en terminal uniquement.
  license :cannot_represent

  on_macos do
    on_arm do
      url "$base/$mac_arm.tar.gz"
      sha256 "$mac_arm_sum"
    end
    on_intel do
      url "$base/$mac_x86.tar.gz"
      sha256 "$mac_x86_sum"
    end
  end

  on_linux do
    on_arm do
      url "$base/$lin_arm.tar.gz"
      sha256 "$lin_arm_sum"
    end
    on_intel do
      url "$base/$lin_x86.tar.gz"
      sha256 "$lin_x86_sum"
    end
  end

  def install
    bin.install "rail"
    doc.install "README.md", "LICENSE"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/rail --version")
  end
end
EOF

# ---------------------------------------------------------------- Scoop
cat > "$out/bucket/rainhill.json" <<EOF
{
    "version": "$version",
    "description": "$desc",
    "homepage": "https://github.com/$repo",
    "license": {
        "identifier": "Rainhill Terminal License",
        "url": "https://github.com/$repo/blob/$tag/LICENSE"
    },
    "architecture": {
        "64bit": {
            "url": "$base/$win_x86.zip",
            "hash": "$win_x86_sum",
            "extract_dir": "$win_x86"
        },
        "arm64": {
            "url": "$base/$win_arm.zip",
            "hash": "$win_arm_sum",
            "extract_dir": "$win_arm"
        }
    },
    "bin": "rail.exe",
    "checkver": {
        "github": "https://github.com/$repo"
    },
    "autoupdate": {
        "architecture": {
            "64bit": {
                "url": "https://github.com/$repo/releases/download/v\$version/rail-v\$version-x86_64-pc-windows-msvc.zip",
                "extract_dir": "rail-v\$version-x86_64-pc-windows-msvc"
            },
            "arm64": {
                "url": "https://github.com/$repo/releases/download/v\$version/rail-v\$version-aarch64-pc-windows-msvc.zip",
                "extract_dir": "rail-v\$version-aarch64-pc-windows-msvc"
            }
        },
        "hash": {
            "url": "\$baseurl/SHA256SUMS.txt"
        }
    }
}
EOF

echo "Homebrew : $out/Formula/rainhill.rb"
echo "Scoop    : $out/bucket/rainhill.json"
