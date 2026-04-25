cask "vmux" do
  version "0.1.0"
  sha256 "PLACEHOLDER"

  url "https://github.com/vmux-ai/vmux/releases/download/v#{version}/Vmux-#{version}-mac.dmg"
  name "Vmux"
  desc "Tiling browser with pane multiplexing"
  homepage "https://github.com/vmux-ai/vmux"

  app "Vmux.app"
end
