cask "vmux" do
  version "0.0.7"
  sha256 "7907e13c78a09edc9db6d52dd629999375c49f99c4b565c568898087120d4bea"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.7/Vmux_0.0.7_aarch64.dmg"
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
