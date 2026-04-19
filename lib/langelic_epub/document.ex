defmodule LangelicEpub.Document do
  @moduledoc """
  An EPUB document — metadata, spine (reading order), assets, and TOC.

  All chapters and assets are fully loaded into memory.
  """

  @type t :: %__MODULE__{
          title: String.t(),
          creators: [String.t()],
          language: String.t() | nil,
          identifier: String.t(),
          publisher: String.t() | nil,
          date: String.t() | nil,
          description: String.t() | nil,
          rights: String.t() | nil,
          metadata: %{String.t() => [String.t()]},
          spine: [LangelicEpub.Chapter.t()],
          assets: [LangelicEpub.Asset.t()],
          toc: [LangelicEpub.NavItem.t()],
          cover_asset_id: String.t() | nil,
          version: String.t()
        }

  defstruct title: "",
            creators: [],
            language: nil,
            identifier: "",
            publisher: nil,
            date: nil,
            description: nil,
            rights: nil,
            metadata: %{},
            spine: [],
            assets: [],
            toc: [],
            cover_asset_id: nil,
            version: "3.0"
end
