cask "vmux" do
  version "0.0.7"
  sha256 "c13e462403fa6f944cd47ae56284676387226787718be5790cfd8c943328e89b"

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
