cask "vmux" do
  version "0.0.11"
  sha256 "03beb6b7e8f60a64a50c7db5a82e54431d865cacc0f53e20dfbf0e5ee9e719d3"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.11/Vmux_0.0.11_aarch64.dmg"
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
