cask "vmux" do
  version "0.0.3"
  sha256 "8a29b8c0fd3c6b1997dffde1b49aa0b8bfa899f17fbc0dd544c693139e99f149"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.3/Vmux_0.0.3_aarch64.dmg"
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
