cask "vmux" do
  version "0.0.9"
  sha256 "b146d184b07c03381b7127c751197c37d79c392a3454fa620c6db56d67e45ed4"

  url "https://github.com/vmux-ai/vmux/releases/download/v0.0.9/Vmux_0.0.9_aarch64.dmg"
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
