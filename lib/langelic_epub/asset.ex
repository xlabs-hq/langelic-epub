defmodule LangelicEpub.Asset do
  @moduledoc """
  A non-chapter resource embedded in the EPUB: stylesheet, font, image,
  or any other supporting file.
  """

  @type t :: %__MODULE__{
          id: String.t(),
          file_name: String.t(),
          media_type: String.t(),
          data: binary()
        }

  defstruct [:id, :file_name, :media_type, :data]
end
