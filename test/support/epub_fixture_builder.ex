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

  @doc """
  Build a minimal EPUB 2 fixture whose OPF structural elements all use the
  `opf:` prefix. Returns the raw bytes.
  """
  @spec namespaced_opf_epub2(keyword()) :: binary()
  def namespaced_opf_epub2(opts \\ []) do
    title = Keyword.get(opts, :title, "Namespaced OPF EPUB")
    language = Keyword.get(opts, :language, "en-GB")
    identifier = Keyword.get(opts, :identifier, "urn:uuid:namespaced-opf-epub-2")
    creator = Keyword.get(opts, :creator, "Jane Doe")

    chapter1_xhtml = ~s"""
    <?xml version="1.0" encoding="UTF-8"?>
    <html xmlns="http://www.w3.org/1999/xhtml" xml:lang="#{language}">
      <head><title>One</title></head>
      <body><h1>One</h1><p>Namespaced chapter one.</p></body>
    </html>
    """

    chapter2_xhtml = ~s"""
    <?xml version="1.0" encoding="UTF-8"?>
    <html xmlns="http://www.w3.org/1999/xhtml" xml:lang="#{language}">
      <head><title>Two &amp; More</title></head>
      <body><h1>Two</h1><p>Namespaced chapter two.</p></body>
    </html>
    """

    ncx_xml = ~s"""
    <?xml version="1.0" encoding="UTF-8"?>
    <ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
      <head>
        <meta name="dtb:uid" content="#{escape(identifier)}"/>
        <meta name="dtb:depth" content="1"/>
        <meta name="dtb:totalPageCount" content="0"/>
        <meta name="dtb:maxPageNumber" content="0"/>
      </head>
      <docTitle><text>#{escape(title)}</text></docTitle>
      <navMap>
        <navPoint id="nav-1" playOrder="1">
          <navLabel><text>One</text></navLabel>
          <content src="text/chapter1.xhtml"/>
        </navPoint>
        <navPoint id="nav-2" playOrder="2">
          <navLabel><text>Two</text></navLabel>
          <content src="text/chapter2.xhtml"/>
        </navPoint>
      </navMap>
    </ncx>
    """

    opf_xml = ~s"""
    <?xml version="1.0" encoding="UTF-8"?>
    <opf:package xmlns:opf="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="book-id">
      <opf:metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
        <dc:identifier id="book-id">#{escape(identifier)}</dc:identifier>
        <dc:title>#{escape(title)}</dc:title>
        <dc:language>#{escape(language)}</dc:language>
        <dc:creator opf:role="aut">#{escape(creator)}</dc:creator>
      </opf:metadata>
      <opf:manifest>
        <opf:item id="ncxtoc" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
        <opf:item id="style" href="styles/book.css" media-type="text/css"/>
        <opf:item id="chapter-one" href="text/chapter1.xhtml" media-type="application/xhtml+xml"/>
        <opf:item id="chapter-two" href="text/chapter2.xhtml" media-type="application/xhtml+xml"/>
      </opf:manifest>
      <opf:spine toc="ncxtoc">
        <opf:itemref idref="chapter-one"/>
        <opf:itemref idref="chapter-two"/>
      </opf:spine>
    </opf:package>
    """

    entries = [
      {~c"mimetype", "application/epub+zip"},
      {~c"META-INF/container.xml", String.trim_leading(@container_xml)},
      {~c"OEBPS/content.opf", opf_xml},
      {~c"OEBPS/toc.ncx", ncx_xml},
      {~c"OEBPS/styles/book.css", "body { font-family: serif; }"},
      {~c"OEBPS/text/chapter1.xhtml", chapter1_xhtml},
      {~c"OEBPS/text/chapter2.xhtml", chapter2_xhtml}
    ]

    {:ok, {_name, bytes}} = :zip.create(~c"namespaced-opf.epub", entries, [:memory])
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
