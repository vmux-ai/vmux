cask "vmux" do
  version "0.0.12"
  sha256 "b3b0b958a37305130d28f73e127b0fe7f258707498af38b0c110b60c28f9d29f"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.12/Vmux_0.0.12_aarch64.dmg"
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
