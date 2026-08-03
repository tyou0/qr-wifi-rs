class QrWifiRs < Formula
  desc "Share Wi-Fi credentials as QR codes and connect from Wi-Fi QR payloads"
  homepage "https://gitea.thetomyou.com/mistercorea/qr_wifi_rs"
  license "MIT"
  url "https://gitea.thetomyou.com/mistercorea/qr_wifi_rs/archive/v0.2.0.tar.gz"
  sha256 "235ac41c57462e7f1b876b47e00ae2dccb4cbcb711c3b32dd7ddca3378063507"
  head "https://gitea.thetomyou.com/mistercorea/qr_wifi_rs.git", branch: "main"

  depends_on :macos
  depends_on "rust" => :build

  def install
    system "cargo", "build", "--release", "--locked",
           "-p", "qr-wifi-cli",
           "-p", "qr-wifi-tui",
           "-p", "qr-wifi-host",
           "-p", "qr-wifi-gui",
           "--features", "qr-wifi-gui/custom-protocol"

    bin.install "target/release/qr-wifi"
    bin.install "target/release/qr-wifi-tui"
    bin.install "target/release/qr-wifi-host"
    bin.install "scripts/install-native-host.sh" => "qr-wifi-install-native-host"

    app_bundle = prefix/"QR Wi-Fi RS.app"
    (app_bundle/"Contents/MacOS").install "target/release/qr-wifi-gui"
    (app_bundle/"Contents/Resources").install "src-tauri/icons/icon.icns"
    (app_bundle/"Contents/Info.plist").write <<~PLIST
      <?xml version="1.0" encoding="UTF-8"?>
      <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
      <plist version="1.0">
      <dict>
        <key>CFBundleDisplayName</key><string>QR Wi-Fi RS</string>
        <key>CFBundleExecutable</key><string>qr-wifi-gui</string>
        <key>CFBundleIconFile</key><string>icon.icns</string>
        <key>CFBundleIdentifier</key><string>com.thetomyou.qrwifirs</string>
        <key>CFBundleName</key><string>QR Wi-Fi RS</string>
        <key>CFBundlePackageType</key><string>APPL</string>
        <key>CFBundleShortVersionString</key><string>#{version}</string>
        <key>NSCameraUsageDescription</key><string>QR Wi-Fi RS uses the camera to scan Wi-Fi QR codes.</string>
      </dict>
      </plist>
    PLIST
    bin.write_exec_script app_bundle/"Contents/MacOS/qr-wifi-gui"
  end

  def caveats
    <<~EOS
      Installed binaries:
        qr-wifi
        qr-wifi-tui
        qr-wifi-host
        qr-wifi-gui

      Desktop application:
        open "#{opt_prefix}/QR Wi-Fi RS.app"

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
    assert_predicate bin/"qr-wifi-gui", :executable?
    assert_predicate bin/"qr-wifi-install-native-host", :executable?
    assert_predicate prefix/"QR Wi-Fi RS.app/Contents/MacOS/qr-wifi-gui", :executable?
  end
end
