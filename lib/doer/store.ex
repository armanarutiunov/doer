defmodule Doer.Store do
  alias Doer.Todo

  @dir Path.expand("~/.doer")
  @path Path.join(@dir, "todos.json")

  def load do
    File.mkdir_p!(@dir)

    case File.read(@path) do
      {:ok, contents} ->
        contents
        |> Jason.decode!()
        |> Enum.map(&Todo.from_map/1)

      {:error, :enoent} ->
        []
    end
  rescue
    _ -> []
  end

  def save(todos) do
    File.mkdir_p!(@dir)
    json = Jason.encode!(todos, pretty: true)
    tmp = @path <> ".tmp"
    File.write!(tmp, json)
    File.rename!(tmp, @path)
  end
end
