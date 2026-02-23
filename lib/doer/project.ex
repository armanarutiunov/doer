defmodule Doer.Project do
  @derive {Jason.Encoder, only: [:id, :name, :index, :parent_id, :children_ids]}
  defstruct [:id, :name, :index, :parent_id, children_ids: []]

  def new(name, index, opts \\ []) do
    %__MODULE__{
      id: :crypto.strong_rand_bytes(8) |> Base.encode16(case: :lower),
      name: name,
      index: index,
      parent_id: opts[:parent_id]
    }
  end

  def from_map(map) when is_map(map) do
    %__MODULE__{
      id: map["id"],
      name: map["name"],
      index: map["index"],
      parent_id: map["parent_id"],
      children_ids: map["children_ids"] || []
    }
  end
end
