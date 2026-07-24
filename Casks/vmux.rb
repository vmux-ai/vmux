cask "vmux" do
  version "0.0.29"
  sha256 "94815b7b36e78cffafb857f47f0b66d9142701c59a045a73c5800599c3601a6c"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.29/Vmux_0.0.29_aarch64.dmg"
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
