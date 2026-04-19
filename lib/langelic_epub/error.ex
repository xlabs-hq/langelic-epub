defmodule LangelicEpub.Error do
  @moduledoc """
  Error returned from `LangelicEpub.parse/1` or `LangelicEpub.build/1`.

  The `kind` field is an atom identifying the error class. The `message` field
  is a human-readable string suitable for logging.

  ## Kinds

  Parse errors:

    * `:invalid_zip` — bytes are not a valid ZIP archive
    * `:missing_container` — no `META-INF/container.xml`
    * `:missing_opf` — OPF file referenced in container.xml not found
    * `:malformed_opf` — OPF could not be parsed
    * `:io` — internal I/O failure

  Build errors:

    * `:missing_required_field` — title, identifier, or language is missing
    * `:invalid_chapter` — a chapter's data is not valid UTF-8 XHTML
    * `:duplicate_id` — two chapters or assets share the same `id`

  Safety:

    * `:panic` — Rust side panicked. This should never happen — report a bug.
  """

  @type kind ::
          :invalid_zip
          | :missing_container
          | :missing_opf
          | :malformed_opf
          | :io
          | :missing_required_field
          | :invalid_chapter
          | :duplicate_id
          | :panic

  @type t :: %__MODULE__{kind: kind(), message: String.t()}

  defexception [:kind, :message]

  @impl true
  def message(%__MODULE__{message: m}), do: m
end
