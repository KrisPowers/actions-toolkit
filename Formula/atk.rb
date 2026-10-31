class Atk < Formula
  desc "Run GitHub Actions-style CI/CD workflows locally in Docker"
  homepage "https://github.com/KrisPowers/actions-toolkit"
  license "MIT"
  version "1.1.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/KrisPowers/actions-toolkit/releases/download/v#{version}/atk-macos-aarch64.tar.gz"
      sha256 "16f592a024a9c6659e66f1ad2612514a745aaffdc93afb55213597d230612421"
    else
      url "https://github.com/KrisPowers/actions-toolkit/releases/download/v#{version}/atk-macos-x86_64.tar.gz"
      sha256 "44b8698f3884eccee8e4d873bf782258e58268cc48cf53ec68a4342e417e9b9b"
    end
  end

  on_linux do
    url "https://github.com/KrisPowers/actions-toolkit/releases/download/v#{version}/atk-linux-x86_64.tar.gz"
    sha256 "caa08ffca0cfdf7c8721ce2e1fc6e6f07534a686230cfb14d4b9141bf5e50cb9"
  end

  def install
    bin.install "atk"
  end

  test do
    system "#{bin}/atk", "--help"
  end
end
