defmodule Doer.Todo do
  @derive {Jason.Encoder, only: [:id, :text, :done, :created_at, :completed_at]}
  defstruct [:id, :text, :done, :created_at, :completed_at, :source]

  def new(text) do
    %__MODULE__{
      id: :crypto.strong_rand_bytes(8) |> Base.encode16(case: :lower),
      text: text,
      done: false,
      created_at: System.system_time(:second),
      completed_at: nil
    }
  end

  def toggle(%__MODULE__{done: true} = todo) do
    %{todo | done: false, completed_at: nil}
  end

  def toggle(%__MODULE__{done: false} = todo) do
    %{todo | done: true, completed_at: System.system_time(:second)}
  end

  def age_label(%__MODULE__{created_at: created_at}) do
    days = div(System.system_time(:second) - created_at, 86400)
    "#{days}d"
  end

  def completed_label(%__MODULE__{completed_at: nil}), do: ""

  def completed_label(%__MODULE__{completed_at: completed_at}) do
    days = div(System.system_time(:second) - completed_at, 86400)
    "#{days}d"
  end

  def from_map(map) when is_map(map) do
    %__MODULE__{
      id: map["id"],
      text: map["text"],
      done: map["done"],
      created_at: map["created_at"],
      completed_at: map["completed_at"]
    }
  end
end
