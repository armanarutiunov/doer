defmodule Doer.Store do
  alias Doer.Todo
  alias Doer.Project

  @dir Path.expand("~/.doer")
  @legacy_path Path.join(@dir, "todos.json")
  @all_todos_path Path.join(@dir, "all-todos.json")
  @projects_dir Path.join(@dir, "projects")

  def init do
    File.mkdir_p!(@dir)
    File.mkdir_p!(@projects_dir)
    migrate()
  end

  defp migrate do
    if File.exists?(@legacy_path) and not File.exists?(@all_todos_path) do
      File.rename!(@legacy_path, @all_todos_path)
    end
  end

  # Legacy load — keep for backwards compat during transition
  def load do
    init()
    load_all_todos()
  end

  def save(todos), do: save_all_todos(todos)

  # All Todos (ungrouped)

  def load_all_todos do
    case File.read(@all_todos_path) do
      {:ok, contents} ->
        contents |> Jason.decode!() |> Enum.map(&Todo.from_map/1)

      {:error, :enoent} ->
        []
    end
  rescue
    _ -> []
  end

  def save_all_todos(todos) do
    write_json(@all_todos_path, todos)
  end

  # Projects

  def load_projects do
    case File.ls(@projects_dir) do
      {:ok, files} ->
        files
        |> Enum.filter(&String.ends_with?(&1, ".json"))
        |> Enum.map(fn file ->
          path = Path.join(@projects_dir, file)
          data = path |> File.read!() |> Jason.decode!()
          project = Project.from_map(data)
          todos = (data["todos"] || []) |> Enum.map(&Todo.from_map/1)
          {project, todos}
        end)
        |> Enum.sort_by(fn {p, _} -> p.index end)

      {:error, :enoent} ->
        []
    end
  rescue
    _ -> []
  end

  def save_project(project, todos) do
    data = %{
      "id" => project.id,
      "name" => project.name,
      "index" => project.index,
      "parent_id" => project.parent_id,
      "children_ids" => project.children_ids,
      "todos" => todos
    }

    path = Path.join(@projects_dir, "#{project.id}.json")
    write_json(path, data)
  end

  def delete_project(project_id) do
    path = Path.join(@projects_dir, "#{project_id}.json")
    File.rm(path)
  end

  defp write_json(path, data) do
    json = Jason.encode!(data, pretty: true)
    tmp = path <> ".tmp"
    File.write!(tmp, json)
    File.rename!(tmp, path)
  end
end
