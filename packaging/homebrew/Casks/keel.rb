cask "keel" do
  arch arm: "aarch64", intel: "x86_64"

  version "0.12.0"
  sha256 arm:   "0000000000000000000000000000000000000000000000000000000000000000",
         intel: "0000000000000000000000000000000000000000000000000000000000000001"

  url "https://github.com/daneb/keel/releases/download/v#{version}/keel-#{version}-#{arch}-apple-darwin.tar.gz"
  name "keel"
  desc "Gated harness for AI-assisted delivery"
  homepage "https://daneb.github.io/keel/"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: :ventura

  binary "keel-#{version}-#{arch}-apple-darwin/keel"
end
