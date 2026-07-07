cask "vmux" do
  version "0.0.22"
  sha256 "056127700925bf6bc22d5ac891a8a27affb46581e279bd2db7f08b6bd83d8237"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.22/Vmux_0.0.22_aarch64.dmg"
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
