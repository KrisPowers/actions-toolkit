class ActionsToolkit < Formula
  desc "Local, self-hosted GitHub Actions-compatible workflow runner"
  homepage "https://github.com/KrisPowers/actions-toolkit"
  license "MIT"
  version "0.1.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/KrisPowers/actions-toolkit/releases/download/v#{version}/actions-toolkit-macos-aarch64.tar.gz"
      sha256 "REPLACE_WITH_MACOS_AARCH64_SHA256"
    else
      url "https://github.com/KrisPowers/actions-toolkit/releases/download/v#{version}/actions-toolkit-macos-x86_64.tar.gz"
      sha256 "REPLACE_WITH_MACOS_X86_64_SHA256"
    end
  end

  on_linux do
    url "https://github.com/KrisPowers/actions-toolkit/releases/download/v#{version}/actions-toolkit-linux-x86_64.tar.gz"
    sha256 "REPLACE_WITH_LINUX_X86_64_SHA256"
  end

  def install
    bin.install "actions-toolkit"
  end

  test do
    system "#{bin}/actions-toolkit", "--help"
  end
end
