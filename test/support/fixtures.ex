defmodule LangelicEpub.Fixtures do
  @moduledoc false

  @fixtures_dir Path.expand("fixtures", __DIR__)
  @samples_dir Path.join(@fixtures_dir, "samples")

  @spec fixtures_dir() :: String.t()
  def fixtures_dir, do: @fixtures_dir

  @spec samples_dir() :: String.t()
  def samples_dir, do: @samples_dir

  @spec path(String.t()) :: String.t()
  def path(name), do: Path.join(@fixtures_dir, name)

  @spec sample_paths() :: [String.t()]
  def sample_paths do
    case File.ls(@samples_dir) do
      {:ok, entries} ->
        entries
        |> Enum.filter(&String.ends_with?(&1, ".epub"))
        |> Enum.sort()
        |> Enum.map(&Path.join(@samples_dir, &1))

      _ ->
        []
    end
  end

  @spec read!(String.t()) :: binary()
  def read!(path), do: File.read!(path)
end
