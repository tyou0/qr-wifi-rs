# Homebrew formula for qr-wifi-rs
#
# This formula installs the QR Wi-Fi RS toolkit:
# - qr-wifi (CLI with interactive menu)
# - qr-wifi-tui (standalone TUI)
# - qr-wifi-host (Native Messaging host)
#
# Installation:
#   brew install qr-wifi-rs
#
# Or from a local tap:
#   brew install --HEAD path/to/qr-wifi-rs/scripts/homebrew-formula.rb

class QrWifiRs < Formula
  desc "Pure-Rust, cross-platform toolkit for sharing Wi-Fi as QR codes"
  homepage "https://github.com/thetomyou/qr-wifi-rs"
  url "https://github.com/thetomyou/qr-wifi-rs/archive/refs/tags/v0.1.0.tar.gz"
  sha256 :any # Will be filled in during release
  license "MIT"

  depends_on "rustup" => :build

  # Install all three binaries
  def install
    # Build the workspace
    system "cargo", "build", "--release", "--workspace"

    # Install each binary
    bin.install "target/release/qr-wifi" => "qr-wifi"
    bin.install "target/release/qr-wifi-tui"
    bin.install "target/release/qr-wifi-host"

    # Generate shell completions (optional)
    # system "cargo", "run", "-p", "qr-wifi-cli", "--", "--generate-completions"
  end

  def caveats
    <<~EOS
      The QR Wi-Fi RS toolkit has been installed with three binaries:

        • qr-wifi       - CLI (run without flags for interactive menu)
        • qr-wifi-tui   - Standalone terminal UI
        • qr-wifi-host  - Native Messaging host for browser extensions

      To set up the browser extension:

        1. Load 'extension/' as an unpacked extension in Chrome/Firefox
        2. Run: scripts/install-native-host.sh --chrome-extension-id <EXTENSION_ID>
        3. Or manually register the native messaging host

      For more information:
        https://github.com/thetomyou/qr-wifi-rs
    EOS
  end

  test do
    # Test that the binaries run and show help
    system bin/"qr-wifi", "--help"
    system bin/"qr-wifi-tui", "--help"  # Will show usage or enter menu

    # Test that the host binary exists and is executable
    assert_predicate bin/"qr-wifi-host", :executable?

    # Test a simple operation (list networks)
    system bin/"qr-wifi", "--list"
  end
end

# Development notes:
#
# To create a release for Homebrew:
#
# 1. Tag the release on GitHub: git tag v0.1.0 && git push --tags
# 2. Create a GitHub release with the tag
# 3. Download the tar.gz and compute sha256: shasum -a 256 qr-wifi-rs-0.1.0.tar.gz
# 4. Update the sha256 field in this formula
# 5. Submit to Homebrew/homebrew-core or create a tap
#
# For HEAD installs (from git):
#   brew install --HEAD https://raw.githubusercontent.com/thetomyou/qr-wifi-rs/main/scripts/homebrew-formula.rb
