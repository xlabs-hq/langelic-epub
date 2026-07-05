defmodule LangelicEpub.Document do
  @moduledoc """
  An EPUB document — metadata, spine (reading order), assets, and TOC.

  All chapters and assets are fully loaded into memory.

  ## Page progression direction

  `page_progression_direction` sets the OPF `<spine page-progression-direction>`
  attribute and, for `"rtl"`, orients the generated nav document (its `<html>`
  root gets `dir="rtl"` plus the document language) so table-of-contents labels
  render in the correct direction. Allowed values:

    * `"rtl"` — right-to-left pagination (Arabic, Hebrew, …)
    * `"ltr"` — left-to-right pagination
    * `nil` — omit the attribute; readers fall back to their default (ltr)

  Any other value makes `LangelicEpub.build/1` return
  `{:error, %LangelicEpub.Error{kind: :invalid_page_direction}}`.

  This field is set from the **target** language at build time. `parse/1` always
  returns `nil` here — a source EPUB's direction is intentionally not
  round-tripped (a `rtl` Japanese source rebuilt into English must shed it).
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
          version: String.t(),
          page_progression_direction: String.t() | nil
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
            version: "3.0",
            page_progression_direction: nil
end
