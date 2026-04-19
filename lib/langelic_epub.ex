defmodule LangelicEpub do
  @moduledoc """
  EPUB read and write for Elixir.

  ## Reading

      {:ok, doc} = LangelicEpub.parse(File.read!("book.epub"))
      doc.title         # => "The Hobbit"
      doc.language      # => "en"
      Enum.count(doc.spine)  # => 23

  ## Writing

      doc = %LangelicEpub.Document{
        title: "Translated Title",
        language: "th",
        creators: ["Original Author"],
        identifier: "urn:uuid:...",
        spine: [
          %LangelicEpub.Chapter{
            id: "ch1",
            file_name: "ch1.xhtml",
            title: "บทที่ 1",
            media_type: "application/xhtml+xml",
            data: chapter_xhtml
          }
        ]
      }

      {:ok, bytes} = LangelicEpub.build(doc)
      File.write!("translated.epub", bytes)
  """

  alias LangelicEpub.{Document, Error}

  @doc """
  Parse EPUB bytes into a `LangelicEpub.Document`.

  Accepts the raw bytes of an EPUB file. All chapters and assets are loaded
  into memory; do not call this on documents larger than your available memory.

  ## Errors

  Returns `{:error, %LangelicEpub.Error{}}` for:

    * `:invalid_zip` — bytes are not a valid ZIP archive
    * `:missing_container` — no `META-INF/container.xml`
    * `:missing_opf` — OPF file referenced in `container.xml` not found
    * `:malformed_opf` — OPF could not be parsed
    * `:io` — internal I/O failure
    * `:panic` — Rust side panicked (should never happen; report a bug)
  """
  @spec parse(binary()) :: {:ok, Document.t()} | {:error, Error.t()}
  def parse(epub_bytes) when is_binary(epub_bytes) do
    LangelicEpub.Native.parse(epub_bytes)
  end

  @doc """
  Build a `LangelicEpub.Document` into EPUB bytes.

  The generated EPUB is always EPUB 3 with a backward-compatible `toc.ncx`.

  ## Errors

  Returns `{:error, %LangelicEpub.Error{}}` for:

    * `:missing_required_field` — title, identifier, or language is missing
    * `:invalid_chapter` — a chapter's `data` is not valid UTF-8 XHTML
    * `:duplicate_id` — two chapters or assets share the same `id`
    * `:panic` — Rust side panicked (report a bug)
  """
  @spec build(Document.t()) :: {:ok, binary()} | {:error, Error.t()}
  def build(%Document{} = doc) do
    LangelicEpub.Native.build(doc)
  end
end
