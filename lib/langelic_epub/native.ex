defmodule LangelicEpub.Native do
  @moduledoc false

  version = Mix.Project.config()[:version]

  use RustlerPrecompiled,
    otp_app: :langelic_epub,
    crate: "langelic_epub",
    base_url:
      "https://github.com/xlabs-hq/langelic-epub/releases/download/v#{version}",
    force_build:
      System.get_env("LANGELIC_EPUB_BUILD") in ["1", "true"] or
        Mix.env() in [:dev, :test],
    targets: ~w(
      aarch64-apple-darwin
      x86_64-apple-darwin
      aarch64-unknown-linux-gnu
      x86_64-unknown-linux-gnu
      x86_64-unknown-linux-musl
    ),
    nif_versions: ["2.16"],
    version: version

  def parse(_bytes), do: :erlang.nif_error(:nif_not_loaded)
  def build(_doc), do: :erlang.nif_error(:nif_not_loaded)
end
