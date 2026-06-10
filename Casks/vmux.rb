cask "vmux" do
  version "0.0.14"
  sha256 "6e2006c23fc488a4d6eef1ef6c513d294ea60bf76a24bc515bdf6a203ce33cc9"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.14/Vmux_0.0.14_aarch64.dmg"
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
