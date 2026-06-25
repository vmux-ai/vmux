cask "vmux" do
  version "0.0.18"
  sha256 "42029205e1b82af3b1cdb166e37a1135885aaf727956b4bef8633bd8baf663e8"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.18/Vmux_0.0.18_aarch64.dmg"
  name "Vmux"
  desc "AI-native workspace combining browser and terminal panes"
  homepage "https://vmux.ai/"

  depends_on macos: :ventura

  app "Vmux.app"

  zap trash: [
    "~/Library/Application Support/ai.vmux.desktop",
    "~/Library/Caches/ai.vmux.desktop",
    "~/Library/Preferences/ai.vmux.desktop.plist",
  ]
end
