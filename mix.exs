defmodule LangelicEpub.MixProject do
  use Mix.Project

  @version "0.1.0"
  @source_url "https://github.com/xlabs-hq/langelic-epub"

  def project do
    [
      app: :langelic_epub,
      version: @version,
      elixir: "~> 1.15",
      elixirc_paths: elixirc_paths(Mix.env()),
      start_permanent: Mix.env() == :prod,
      deps: deps(),
      package: package(),
      docs: docs(),
      description: "EPUB read and write for Elixir, backed by a Rustler NIF.",
      source_url: @source_url,
      dialyzer: [
        plt_core_path: "priv/plts",
        plt_file: {:no_warn, "priv/plts/project.plt"},
        flags: [:error_handling, :unknown, :underspecs]
      ]
    ]
  end

  def application, do: [extra_applications: [:logger]]

  defp elixirc_paths(:test), do: ["lib", "test/support"]
  defp elixirc_paths(_), do: ["lib"]

  defp deps do
    [
      {:rustler_precompiled, "~> 0.8"},
      {:rustler, "~> 0.37", optional: true},
      {:ex_doc, "~> 0.34", only: :dev, runtime: false},
      {:credo, "~> 1.7", only: [:dev, :test], runtime: false},
      {:dialyxir, "~> 1.4", only: [:dev, :test], runtime: false}
    ]
  end

  defp package do
    [
      files: ~w(
        lib
        native/langelic_epub/src
        native/langelic_epub/Cargo.toml
        native/langelic_epub/.cargo
        checksum-*.exs
        mix.exs
        README.md
        CHANGELOG.md
        LICENSE
        NOTICE
      ),
      licenses: ["MIT"],
      links: %{
        "GitHub" => @source_url,
        "Changelog" => "#{@source_url}/blob/main/CHANGELOG.md"
      }
    ]
  end

  defp docs do
    [
      main: "LangelicEpub",
      extras: ["README.md", "CHANGELOG.md"],
      source_ref: "v#{@version}"
    ]
  end
end
