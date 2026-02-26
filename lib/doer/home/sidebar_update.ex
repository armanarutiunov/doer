defmodule Doer.Home.SidebarUpdate do
  alias Doer.{Project, Store}
  alias Doer.Home.Helpers

  # Guards for cursor=0 (All Todos) — project-only operations
  def update(:sidebar_rename_project, %{sidebar_cursor: 0}), do: :noreply
  def update(:sidebar_delete_project, %{sidebar_cursor: 0}), do: :noreply
  def update(:sidebar_add_subproject, %{sidebar_cursor: 0}), do: :noreply
  def update(:sidebar_reorder_down, %{sidebar_cursor: 0}), do: :noreply
  def update(:sidebar_reorder_up, %{sidebar_cursor: 0}), do: :noreply

  def update(:sidebar_down, state) do
    max = sidebar_item_count(state) - 1
    new_cursor = min(state.sidebar_cursor + 1, max)
    state = %{state | sidebar_cursor: new_cursor}
    {switch_view_for_cursor(state)}
  end

  def update(:sidebar_up, state) do
    new_cursor = max(state.sidebar_cursor - 1, 0)
    state = %{state | sidebar_cursor: new_cursor}
    {switch_view_for_cursor(state)}
  end

  def update(:sidebar_select, state) do
    {%{state | focus: :main}}
  end

  # Add project
  def update(:sidebar_add_project, state) do
    {%{state | sidebar_mode: :insert, sidebar_editing_text: "", sidebar_editing_id: nil}}
  end

  # Add subproject — only on top-level projects
  def update(:sidebar_add_subproject, state) do
    flat = Helpers.flat_ordered_projects(state.projects)

    case Enum.at(flat, state.sidebar_cursor - 1) do
      %Project{parent_id: nil} = parent ->
        {%{state | sidebar_mode: :insert, sidebar_editing_text: "", sidebar_editing_id: {:new_child, parent.id}}}

      _ ->
        :noreply
    end
  end

  # Rename project
  def update(:sidebar_rename_project, state) do
    flat = Helpers.flat_ordered_projects(state.projects)

    case Enum.at(flat, state.sidebar_cursor - 1) do
      %Project{} = project ->
        {%{state | sidebar_mode: :insert, sidebar_editing_text: project.name, sidebar_editing_id: project.id}}

      _ ->
        :noreply
    end
  end

  # Delete project
  def update(:sidebar_delete_project, state) do
    flat = Helpers.flat_ordered_projects(state.projects)

    case Enum.at(flat, state.sidebar_cursor - 1) do
      %Project{} = project ->
        if has_uncompleted_todos?(state, project) do
          {%{state | sidebar_mode: :confirm_delete, sidebar_confirm_project_id: project.id}}
        else
          do_delete_project(state, project)
        end

      _ ->
        :noreply
    end
  end

  def update(:sidebar_confirm_delete, state) do
    project = Enum.find(state.projects, &(&1.id == state.sidebar_confirm_project_id))

    if project do
      {state} = do_delete_project(state, project)
      {%{state | sidebar_mode: :normal, sidebar_confirm_project_id: nil}}
    else
      {%{state | sidebar_mode: :normal, sidebar_confirm_project_id: nil}}
    end
  end

  def update(:sidebar_cancel_delete, state) do
    {%{state | sidebar_mode: :normal, sidebar_confirm_project_id: nil}}
  end

  # Confirm edit (add or rename)
  def update(:sidebar_confirm_edit, state) do
    name = String.trim(state.sidebar_editing_text)

    cond do
      name == "" ->
        {%{state | sidebar_mode: :normal, sidebar_editing_text: "", sidebar_editing_id: nil}}

      # New project
      state.sidebar_editing_id == nil ->
        index = length(Enum.filter(state.projects, &is_nil(&1.parent_id)))
        project = Project.new(name, index)
        Store.save_project(project, [])

        new_cursor = length(Helpers.flat_ordered_projects(state.projects)) + 1

        {%{state |
          sidebar_mode: :normal,
          sidebar_editing_text: "",
          sidebar_editing_id: nil,
          projects: state.projects ++ [project],
          project_todos: Map.put(state.project_todos, project.id, []),
          sidebar_cursor: new_cursor
        }}

      # New child project
      match?({:new_child, _}, state.sidebar_editing_id) ->
        {:new_child, parent_id} = state.sidebar_editing_id
        child_count = Enum.count(state.projects, &(&1.parent_id == parent_id))
        child = Project.new(name, child_count, parent_id: parent_id)

        projects = state.projects ++ [child]

        Store.save_project(child, [])

        flat = Helpers.flat_ordered_projects(projects)
        new_cursor = (Enum.find_index(flat, &(&1.id == child.id)) || 0) + 1

        {%{state |
          sidebar_mode: :normal,
          sidebar_editing_text: "",
          sidebar_editing_id: nil,
          projects: projects,
          project_todos: Map.put(state.project_todos, child.id, []),
          sidebar_cursor: new_cursor
        }}

      # Rename existing project
      is_binary(state.sidebar_editing_id) ->
        project = Enum.find(state.projects, &(&1.id == state.sidebar_editing_id))

        if project do
          updated = %{project | name: name}
          projects = Enum.map(state.projects, fn p -> if p.id == updated.id, do: updated, else: p end)
          Store.save_project(updated, Map.get(state.project_todos, updated.id, []))

          {%{state |
            sidebar_mode: :normal,
            sidebar_editing_text: "",
            sidebar_editing_id: nil,
            projects: projects
          }}
        else
          {%{state | sidebar_mode: :normal, sidebar_editing_text: "", sidebar_editing_id: nil}}
        end
    end
  end

  def update(:sidebar_cancel_edit, state) do
    {%{state | sidebar_mode: :normal, sidebar_editing_text: "", sidebar_editing_id: nil}}
  end

  def update(:sidebar_backspace, state) do
    t = String.slice(state.sidebar_editing_text, 0..-2//1)
    {%{state | sidebar_editing_text: t}}
  end

  def update({:sidebar_type_char, char}, state) do
    {%{state | sidebar_editing_text: state.sidebar_editing_text <> char}}
  end

  # Reorder down
  def update(:sidebar_reorder_down, state) do
    flat = Helpers.flat_ordered_projects(state.projects)

    case Enum.at(flat, state.sidebar_cursor - 1) do
      %Project{parent_id: nil} = project ->
        reorder_parent(state, project, :down)

      %Project{parent_id: pid} = child when not is_nil(pid) ->
        reorder_child(state, child, :down)

      _ ->
        :noreply
    end
  end

  # Reorder up
  def update(:sidebar_reorder_up, state) do
    flat = Helpers.flat_ordered_projects(state.projects)

    case Enum.at(flat, state.sidebar_cursor - 1) do
      %Project{parent_id: nil} = project ->
        reorder_parent(state, project, :up)

      %Project{parent_id: pid} = child when not is_nil(pid) ->
        reorder_child(state, child, :up)

      _ ->
        :noreply
    end
  end

  # --- Helpers ---

  defp switch_view_for_cursor(state) do
    flat = Helpers.flat_ordered_projects(state.projects)

    new_view =
      if state.sidebar_cursor == 0 do
        :all
      else
        case Enum.at(flat, state.sidebar_cursor - 1) do
          nil -> state.current_view
          project -> {:project, project.id}
        end
      end

    switch_view(state, new_view)
  end

  defp sidebar_item_count(state) do
    1 + length(Helpers.flat_ordered_projects(state.projects))
  end

  defp switch_view(state, new_view) do
    if new_view == state.current_view do
      state
    else
      # Save current view state
      saved = %{
        cursor: state.cursor,
        scroll_offset: state.scroll_offset,
        visual_anchor: state.visual_anchor,
        search_text: state.search_text,
        search_matches: state.search_matches
      }

      view_states = Map.put(state.view_states, state.current_view, saved)

      # Load todos for new view
      todos = load_view_todos(state, new_view)

      # Restore saved state or defaults
      restored = Map.get(view_states, new_view, %{})

      %{state |
        current_view: new_view,
        view_states: view_states,
        todos: todos,
        cursor: Map.get(restored, :cursor, 0),
        scroll_offset: Map.get(restored, :scroll_offset, 0),
        visual_anchor: Map.get(restored, :visual_anchor, 0),
        search_text: Map.get(restored, :search_text, ""),
        search_matches: Map.get(restored, :search_matches, []),
        mode: :normal
      }
    end
  end

  defp load_view_todos(state, :all) do
    ungrouped = Store.load_all_todos()
    flat = Helpers.flat_ordered_projects(state.projects)

    Enum.reduce(flat, ungrouped, fn project, todos ->
      ptodos =
        Map.get(state.project_todos, project.id, [])
        |> Enum.map(&%{&1 | source: project.id})

      todos ++ ptodos
    end)
  end

  defp load_view_todos(state, {:project, id}) do
    Map.get(state.project_todos, id, [])
  end

  # Public helper: find which project a cursor position belongs to in All Todos view
  def section_for_cursor(state, cursor) do
    if state.current_view != :all do
      nil
    else
      active = Enum.filter(state.todos, &(!&1.done))

      if cursor >= 0 and cursor < length(active) do
        todo = Enum.at(active, cursor)
        todo.source
      else
        nil
      end
    end
  end

  def section_label(_projects, %Project{parent_id: nil} = project) do
    "# #{project.name}"
  end

  def section_label(projects, %Project{parent_id: pid} = project) do
    parent = Enum.find(projects, &(&1.id == pid))
    if parent, do: "# #{parent.name} / #{project.name}", else: "# #{project.name}"
  end

  defp has_uncompleted_todos?(state, project) do
    ids = [project.id | get_descendant_ids(state.projects, project.id)]

    Enum.any?(ids, fn id ->
      state.project_todos
      |> Map.get(id, [])
      |> Enum.any?(&(!&1.done))
    end)
  end

  defp get_descendant_ids(projects, parent_id) do
    children = Enum.filter(projects, &(&1.parent_id == parent_id))
    child_ids = Enum.map(children, & &1.id)
    child_ids ++ Enum.flat_map(child_ids, &get_descendant_ids(projects, &1))
  end

  defp do_delete_project(state, project) do
    ids_to_delete = [project.id | get_descendant_ids(state.projects, project.id)]

    Enum.each(ids_to_delete, &Store.delete_project/1)

    projects = Enum.reject(state.projects, &(&1.id in ids_to_delete))

    # Save parent if child was deleted (parent still exists)
    if project.parent_id do
      parent = Enum.find(projects, &(&1.id == project.parent_id))
      if parent, do: Store.save_project(parent, Map.get(state.project_todos, parent.id, []))
    end

    project_todos = Map.drop(state.project_todos, ids_to_delete)

    current_view =
      case state.current_view do
        {:project, id} -> if id in ids_to_delete, do: :all, else: state.current_view
        other -> other
      end

    state = %{state |
      projects: projects,
      project_todos: project_todos,
      sidebar_mode: :normal,
      sidebar_confirm_project_id: nil
    }

    max_cursor = max(sidebar_item_count(state) - 1, 0)
    state = %{state | sidebar_cursor: min(state.sidebar_cursor, max_cursor)}

    state = if current_view != state.current_view, do: switch_view(state, current_view), else: state

    {state}
  end

  defp reorder_parent(state, project, direction) do
    parents =
      state.projects
      |> Enum.filter(&is_nil(&1.parent_id))
      |> Enum.sort_by(& &1.index)

    idx = Enum.find_index(parents, &(&1.id == project.id))
    swap_idx = if direction == :down, do: idx + 1, else: idx - 1

    if swap_idx < 0 or swap_idx >= length(parents) do
      :noreply
    else
      swap = Enum.at(parents, swap_idx)
      updated_project = %{project | index: swap.index}
      updated_swap = %{swap | index: project.index}

      projects =
        Enum.map(state.projects, fn p ->
          cond do
            p.id == project.id -> updated_project
            p.id == swap.id -> updated_swap
            true -> p
          end
        end)

      Store.save_project(updated_project, Map.get(state.project_todos, project.id, []))
      Store.save_project(updated_swap, Map.get(state.project_todos, swap.id, []))

      flat = Helpers.flat_ordered_projects(projects)
      new_cursor = (Enum.find_index(flat, &(&1.id == project.id)) || 0) + 1

      {%{state | projects: projects, sidebar_cursor: new_cursor}}
    end
  end

  defp reorder_child(state, child, direction) do
    siblings =
      state.projects
      |> Enum.filter(&(&1.parent_id == child.parent_id))
      |> Enum.sort_by(& &1.index)

    idx = Enum.find_index(siblings, &(&1.id == child.id))
    swap_idx = if direction == :down, do: idx + 1, else: idx - 1

    if swap_idx < 0 or swap_idx >= length(siblings) do
      :noreply
    else
      swap = Enum.at(siblings, swap_idx)
      updated_child = %{child | index: swap.index}
      updated_swap = %{swap | index: child.index}

      projects =
        Enum.map(state.projects, fn p ->
          cond do
            p.id == child.id -> updated_child
            p.id == swap.id -> updated_swap
            true -> p
          end
        end)

      Store.save_project(updated_child, Map.get(state.project_todos, child.id, []))
      Store.save_project(updated_swap, Map.get(state.project_todos, swap.id, []))

      flat = Helpers.flat_ordered_projects(projects)
      new_cursor = (Enum.find_index(flat, &(&1.id == child.id)) || 0) + 1

      {%{state | projects: projects, sidebar_cursor: new_cursor}}
    end
  end
end
