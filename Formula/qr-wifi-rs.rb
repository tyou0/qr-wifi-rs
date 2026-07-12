class QrWifiRs < Formula
  desc "Share Wi-Fi credentials as QR codes and connect from Wi-Fi QR payloads"
  homepage "https://gitea.thetomyou.com/mistercorea/qr_wifi_rs"
  license "MIT"
  url "https://gitea.thetomyou.com/mistercorea/qr_wifi_rs/archive/v0.2.0.tar.gz"
  sha256 "235ac41c57462e7f1b876b47e00ae2dccb4cbcb711c3b32dd7ddca3378063507"
  head "https://gitea.thetomyou.com/mistercorea/qr_wifi_rs.git", branch: "main"

  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release", "--locked",
           "-p", "qr-wifi-cli",
           "-p", "qr-wifi-tui",
           "-p", "qr-wifi-host"

    bin.install "target/release/qr-wifi"
    bin.install "target/release/qr-wifi-tui"
    bin.install "target/release/qr-wifi-host"
    bin.install "scripts/install-native-host.sh" => "qr-wifi-install-native-host"
  end

  def caveats
    <<~EOS
      Installed binaries:
        qr-wifi
        qr-wifi-tui
        qr-wifi-host

      To register the browser Native Messaging host after installing the
      Chrome/Firefox extension:

        qr-wifi-install-native-host --skip-build --host-path "#{opt_bin}/qr-wifi-host" --chrome-extension-id <EXTENSION_ID>

      Firefox does not require a Chrome extension ID:

        qr-wifi-install-native-host --skip-build --host-path "#{opt_bin}/qr-wifi-host"
    EOS
  end

  test do
    assert_match "Usage", shell_output("#{bin}/qr-wifi --help")
    assert_predicate bin/"qr-wifi-tui", :executable?
    assert_predicate bin/"qr-wifi-host", :executable?
    assert_predicate bin/"qr-wifi-install-native-host", :executable?
  end
end
