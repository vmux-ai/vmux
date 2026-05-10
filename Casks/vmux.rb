cask "vmux" do
  version "0.0.4"
  sha256 "33d95700f2a244447414969a13a5ce956e6467cfd0338fc7266013b497b379a2"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.4/Vmux_0.0.4_aarch64.dmg"
  name "Vmux"
  desc "AI-native workspace combining browser and terminal panes"
  homepage "https://vmux.ai"

  depends_on macos: ">= :ventura"

  app "Vmux.app"

  zap trash: [
    "~/Library/Application Support/ai.vmux.desktop",
    "~/Library/Caches/ai.vmux.desktop",
    "~/Library/Preferences/ai.vmux.desktop.plist",
  ]
end
