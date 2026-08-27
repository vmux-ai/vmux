cask "vmux" do
  version "0.0.32"
  sha256 "280a18892e4f3b625869273aac071fc79fe2813128ee329181fb686cd82ff8aa"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.32/Vmux_0.0.32_aarch64.dmg"
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
