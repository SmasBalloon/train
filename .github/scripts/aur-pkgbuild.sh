#!/usr/bin/env bash
# Usage : aur-pkgbuild.sh vX.Y.Z PKGREL chemin/vers/PKGBUILD
# Met le PKGBUILD de rainhill-bin à la version d'une release publiée : pkgver,
# pkgrel et sommes SHA256 des archives Linux, lues dans le SHA256SUMS.txt de la
# release.
set -euo pipefail

tag="${1:?tag manquant (ex. v1.0.0)}"
pkgrel="${2:-1}"
pkgbuild="${3:?chemin du PKGBUILD manquant}"
repo="${GITHUB_REPOSITORY:-SmasBalloon/train}"

[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "Tag invalide : $tag" >&2; exit 1; }
[[ "$pkgrel" =~ ^[1-9][0-9]*$ ]] || { echo "pkgrel invalide : $pkgrel" >&2; exit 1; }
version="${tag#v}"

sums=$(curl -fsSL "https://github.com/$repo/releases/download/$tag/SHA256SUMS.txt")
sum_for() {
  printf '%s\n' "$sums" | awk -v f="rail-$tag-$1-unknown-linux-gnu.tar.gz" '$2 == f { print $1 }'
}
x86=$(sum_for x86_64)
arm=$(sum_for aarch64)
[[ "$x86" =~ ^[0-9a-f]{64}$ && "$arm" =~ ^[0-9a-f]{64}$ ]] \
  || { echo "Sommes des archives Linux introuvables dans SHA256SUMS.txt de $tag" >&2; exit 1; }

sed -i \
  -e "s/^pkgver=.*/pkgver=$version/" \
  -e "s/^pkgrel=.*/pkgrel=$pkgrel/" \
  -e "s/^sha256sums_x86_64=.*/sha256sums_x86_64=('$x86')/" \
  -e "s/^sha256sums_aarch64=.*/sha256sums_aarch64=('$arm')/" \
  "$pkgbuild"

echo "rainhill-bin $version-$pkgrel"
grep -E '^(pkgver|pkgrel|sha256sums_)' "$pkgbuild"
