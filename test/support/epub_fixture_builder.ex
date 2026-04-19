defmodule LangelicEpub.EpubFixtureBuilder do
  @moduledoc false

  @container_xml """
  <?xml version="1.0" encoding="UTF-8"?>
  <container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
    <rootfiles>
      <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
    </rootfiles>
  </container>
  """

  @doc """
  Build a minimal EPUB 3 fixture in memory. Returns the raw bytes.
  """
  @spec minimal_epub3(keyword()) :: binary()
  def minimal_epub3(opts \\ []) do
    title = Keyword.get(opts, :title, "Minimal EPUB 3")
    language = Keyword.get(opts, :language, "en")
    identifier = Keyword.get(opts, :identifier, "urn:uuid:minimal-epub-3")
    creators = Keyword.get(opts, :creators, ["Jane Doe"])
    rights = Keyword.get(opts, :rights, "CC0 1.0")

    chapter_xhtml = ~s"""
    <?xml version="1.0" encoding="UTF-8"?>
    <html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" xml:lang="#{language}">
      <head><title>Chapter 1</title></head>
      <body><h1>Chapter 1</h1><p>Hello, world.</p></body>
    </html>
    """

    nav_xhtml = ~s"""
    <?xml version="1.0" encoding="UTF-8"?>
    <html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
      <head><title>Navigation</title></head>
      <body>
        <nav epub:type="toc" id="toc">
          <h1>Contents</h1>
          <ol><li><a href="chapter1.xhtml">Chapter 1</a></li></ol>
        </nav>
      </body>
    </html>
    """

    creator_xml =
      creators
      |> Enum.with_index()
      |> Enum.map(fn {name, i} ->
        ~s|    <dc:creator id="creator-#{i}">#{escape(name)}</dc:creator>|
      end)
      |> Enum.join("\n")

    opf_xml = ~s"""
    <?xml version="1.0" encoding="UTF-8"?>
    <package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="book-id">
      <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
        <dc:identifier id="book-id">#{escape(identifier)}</dc:identifier>
        <dc:title>#{escape(title)}</dc:title>
        <dc:language>#{escape(language)}</dc:language>
    #{creator_xml}
        <dc:rights>#{escape(rights)}</dc:rights>
        <meta property="dcterms:modified">2026-04-19T00:00:00Z</meta>
      </metadata>
      <manifest>
        <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
        <item id="chapter1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
      </manifest>
      <spine>
        <itemref idref="chapter1"/>
      </spine>
    </package>
    """

    entries = [
      {~c"mimetype", "application/epub+zip"},
      {~c"META-INF/container.xml", String.trim_leading(@container_xml)},
      {~c"OEBPS/content.opf", opf_xml},
      {~c"OEBPS/nav.xhtml", nav_xhtml},
      {~c"OEBPS/chapter1.xhtml", chapter_xhtml}
    ]

    {:ok, {_name, bytes}} = :zip.create(~c"fixture.epub", entries, [:memory])
    bytes
  end

  defp escape(s) do
    s
    |> String.replace("&", "&amp;")
    |> String.replace("<", "&lt;")
    |> String.replace(">", "&gt;")
    |> String.replace("\"", "&quot;")
  end
end
