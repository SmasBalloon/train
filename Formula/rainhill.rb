# Généré par .github/scripts/package-manifests.sh (SmasBalloon/train) : ne pas modifier à la main.
class Rainhill < Formula
  desc "Jeu incrémental de réseau ferroviaire dans le terminal, de 1825 à demain"
  homepage "https://github.com/SmasBalloon/train"
  version "1.0.2"
  # Licence maison : usage et modifications libres, en terminal uniquement.
  license :cannot_represent

  on_macos do
    on_arm do
      url "https://github.com/SmasBalloon/train/releases/download/v1.0.2/rail-v1.0.2-aarch64-apple-darwin.tar.gz"
      sha256 "4b7eb29746836dfc5c0557767e56852f2fab35d1cf39f8233b7d45a46fc1d8bf"
    end
    on_intel do
      url "https://github.com/SmasBalloon/train/releases/download/v1.0.2/rail-v1.0.2-x86_64-apple-darwin.tar.gz"
      sha256 "b0ba94b1cc5cc3a3043ad691b29ee2de5a428ef606bbcae12bf5bfb6534a8938"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/SmasBalloon/train/releases/download/v1.0.2/rail-v1.0.2-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "85845d4735c730aa5b99c72b6f983fc594d7453e4f99409ab548f78be3f30129"
    end
    on_intel do
      url "https://github.com/SmasBalloon/train/releases/download/v1.0.2/rail-v1.0.2-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "f67aca128ce1a167ac077147289ca78a9fc8a57693d9c2eb78fc0183449f59e1"
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
