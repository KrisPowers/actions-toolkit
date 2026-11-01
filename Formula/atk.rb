class Atk < Formula
  desc "Run GitHub Actions-style CI/CD workflows locally in Docker"
  homepage "https://github.com/KrisPowers/actions-toolkit"
  license "MIT"
  version "1.2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/KrisPowers/actions-toolkit/releases/download/v#{version}/atk-macos-aarch64.tar.gz"
      sha256 "fe3ded1444d61411eab1e354b53a6126aad78b6b77a9dcd37b227b5868a380cc"
    else
      url "https://github.com/KrisPowers/actions-toolkit/releases/download/v#{version}/atk-macos-x86_64.tar.gz"
      sha256 "a8a4f3cfbae52b915ffc756da959a0e732f442a096d48a57879a4172fdac61a2"
    end
  end

  on_linux do
    url "https://github.com/KrisPowers/actions-toolkit/releases/download/v#{version}/atk-linux-x86_64.tar.gz"
    sha256 "c5d905081d7e62ffb5e75881cf560073c217ffac7e4c15d4f3d1a2bec4a9b763"
  end

  def install
    bin.install "atk"
  end

  test do
    system "#{bin}/atk", "--help"
  end
end
