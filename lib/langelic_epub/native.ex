defmodule LangelicEpub.Native do
  @moduledoc false

  version = Mix.Project.config()[:version]

  # No published GitHub release yet — build from source until v0.1.0 binaries
  # are actually attached. This can flip back to opt-in once release artifacts
  # exist and rustler_precompiled can resolve them.
  use RustlerPrecompiled,
    otp_app: :langelic_epub,
    crate: "langelic_epub",
    base_url: "https://github.com/xlabs-hq/langelic-epub/releases/download/v#{version}",
    force_build: true,
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
