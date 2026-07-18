defmodule LangelicEpub.RenditionLayoutTest do
  use ExUnit.Case, async: true

  alias LangelicEpub.Chapter
  alias LangelicEpub.Document
  alias LangelicEpub.Error

  defp base_doc(opts) do
    %Document{
      title: "Rendition Layout Test",
      identifier: "urn:uuid:rendition-layout-test",
      language: "en",
      creators: ["Tester"],
      rendition_layout: Keyword.get(opts, :rendition_layout, nil),
      spine: [
        %Chapter{
          id: "page1",
          file_name: "page1.xhtml",
          title: "Page One",
          media_type: "application/xhtml+xml",
          data:
            Keyword.get(
              opts,
              :chapter_data,
              ~s|<?xml version="1.0" encoding="UTF-8"?>\n<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Page One</title><meta name="viewport" content="width=900, height=1350"/></head><body><p>Page one.</p></body></html>|
            )
        }
      ]
    }
  end

  defp opf(bytes) do
    {:ok, handle} = :zip.zip_open(bytes, [:memory])
    {:ok, {_, opf}} = :zip.zip_get(~c"OEBPS/content.opf", handle)
    :zip.zip_close(handle)
    to_string(opf)
  end

  defp chapter_without_viewport do
    ~s|<?xml version="1.0" encoding="UTF-8"?>\n<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Page One</title></head><body><p>Page one.</p></body></html>|
  end

  test "pre-paginated emits rendition:layout metadata exactly once" do
    {:ok, bytes} = LangelicEpub.build(base_doc(rendition_layout: "pre-paginated"))
    opf = opf(bytes)

    assert opf =~ ~s|<meta property="rendition:layout">pre-paginated</meta>|
    assert length(Regex.scan(~r/rendition:layout/, opf)) == 1
  end

  test "reflowable emits explicit rendition:layout metadata" do
    {:ok, bytes} =
      LangelicEpub.build(
        base_doc(rendition_layout: "reflowable", chapter_data: chapter_without_viewport())
      )

    assert opf(bytes) =~ ~s|<meta property="rendition:layout">reflowable</meta>|
  end

  test "nil omits all rendition metadata" do
    {:ok, bytes} =
      LangelicEpub.build(
        base_doc(rendition_layout: nil, chapter_data: chapter_without_viewport())
      )

    refute opf(bytes) =~ "rendition:"
  end

  describe "validation" do
    for invalid <- ["fixed", "PRE-PAGINATED"] do
      test "rejects #{invalid}" do
        assert {:error, %Error{kind: :invalid_rendition_layout}} =
                 LangelicEpub.build(base_doc(rendition_layout: unquote(invalid)))
      end
    end

    test "pre-paginated XHTML chapters must declare a viewport" do
      assert {:error, %Error{kind: :missing_viewport, message: message}} =
               LangelicEpub.build(
                 base_doc(
                   rendition_layout: "pre-paginated",
                   chapter_data: chapter_without_viewport()
                 )
               )

      assert message =~ "page1"
    end
  end

  test "parse/1 does not populate rendition_layout from a source book" do
    {:ok, bytes} = LangelicEpub.build(base_doc(rendition_layout: "pre-paginated"))
    {:ok, parsed} = LangelicEpub.parse(bytes)

    assert parsed.rendition_layout == nil
    assert parsed.title == "Rendition Layout Test"
    assert length(parsed.spine) == 1
  end

  test "the default struct leaves rendition_layout nil" do
    assert %Document{}.rendition_layout == nil
  end
end
