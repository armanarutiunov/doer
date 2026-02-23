defmodule Doer.Home.SidebarUpdate do
  alias Doer.{Project, Store}
  alias Doer.Home.View

  def update(:sidebar_down, state) do
    max = sidebar_item_count(state) - 1
    {%{state | sidebar_cursor: min(state.sidebar_cursor + 1, max)}}
  end

  def update(:sidebar_up, state) do
    {%{state | sidebar_cursor: max(state.sidebar_cursor - 1, 0)}}
  end

  def update(:sidebar_select, state) do
    flat = View.flat_ordered_projects(state.projects)

    new_view =
      if state.sidebar_cursor == 0 do
        :all
      else
        project = Enum.at(flat, state.sidebar_cursor - 1)
        if project, do: {:project, project.id}, else: state.current_view
      end

    state = switch_view(state, new_view)
    {%{state | focus: :main}}
  end

  # Add project
  def update(:sidebar_add_project, state) do
    {%{state | sidebar_mode: :insert, sidebar_editing_text: "", sidebar_editing_id: nil}}
  end

  # Add subproject — only on top-level projects
  def update(:sidebar_add_subproject, state) do
    flat = View.flat_ordered_projects(state.projects)

    case Enum.at(flat, state.sidebar_cursor - 1) do
      %Project{parent_id: nil} = parent ->
        {%{state | sidebar_mode: :insert, sidebar_editing_text: "", sidebar_editing_id: {:new_child, parent.id}}}

      _ ->
        :noreply
    end
  end

  # Rename project
  def update(:sidebar_rename_project, state) do
    flat = View.flat_ordered_projects(state.projects)

    case Enum.at(flat, state.sidebar_cursor - 1) do
      %Project{} = project ->
        {%{state | sidebar_mode: :insert, sidebar_editing_text: project.name, sidebar_editing_id: project.id}}

      _ ->
        :noreply
    end
  end

  # Delete project
  def update(:sidebar_delete_project, state) do
    flat = View.flat_ordered_projects(state.projects)

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

        new_cursor = length(View.flat_ordered_projects(state.projects)) + 1

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
        parent = Enum.find(state.projects, &(&1.id == parent_id))
        child = Project.new(name, 0, parent_id: parent_id)
        updated_parent = %{parent | children_ids: parent.children_ids ++ [child.id]}

        projects =
          state.projects
          |> Enum.map(fn p -> if p.id == parent_id, do: updated_parent, else: p end)
          |> Kernel.++([child])

        Store.save_project(child, [])
        Store.save_project(updated_parent, Map.get(state.project_todos, parent_id, []))

        flat = View.flat_ordered_projects(projects)
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
    flat = View.flat_ordered_projects(state.projects)

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
    flat = View.flat_ordered_projects(state.projects)

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

  defp sidebar_item_count(state) do
    # 0 = All Todos, 1..n = projects
    1 + length(View.flat_ordered_projects(state.projects))
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
      {todos, todo_sources} = load_view_todos(state, new_view)

      # Restore saved state or defaults
      restored = Map.get(view_states, new_view, %{})

      %{state |
        current_view: new_view,
        view_states: view_states,
        todos: todos,
        todo_sources: todo_sources,
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
    flat = View.flat_ordered_projects(state.projects)

    {all_todos, sources} =
      Enum.reduce(flat, {ungrouped, List.duplicate(nil, length(ungrouped))}, fn project, {todos, srcs} ->
        ptodos = Map.get(state.project_todos, project.id, [])
        {todos ++ ptodos, srcs ++ List.duplicate(project.id, length(ptodos))}
      end)

    {all_todos, sources}
  end

  defp load_view_todos(state, {:project, id}) do
    todos = Map.get(state.project_todos, id, [])
    {todos, []}
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

    # Delete files
    Enum.each(ids_to_delete, &Store.delete_project/1)

    # Remove from parent's children_ids if child
    projects =
      if project.parent_id do
        Enum.map(state.projects, fn p ->
          if p.id == project.parent_id do
            %{p | children_ids: Enum.reject(p.children_ids, &(&1 == project.id))}
          else
            p
          end
        end)
      else
        state.projects
      end

    # Remove deleted projects
    projects = Enum.reject(projects, &(&1.id in ids_to_delete))

    # Save parent if child was deleted
    if project.parent_id do
      parent = Enum.find(projects, &(&1.id == project.parent_id))
      if parent, do: Store.save_project(parent, Map.get(state.project_todos, parent.id, []))
    end

    # Clean project_todos
    project_todos = Map.drop(state.project_todos, ids_to_delete)

    # Switch view if deleted project was current
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

    # Clamp cursor
    max_cursor = max(sidebar_item_count(state) - 1, 0)
    state = %{state | sidebar_cursor: min(state.sidebar_cursor, max_cursor)}

    # Switch view if needed
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

      # Update sidebar cursor
      flat = View.flat_ordered_projects(projects)
      new_cursor = (Enum.find_index(flat, &(&1.id == project.id)) || 0) + 1

      {%{state | projects: projects, sidebar_cursor: new_cursor}}
    end
  end

  defp reorder_child(state, child, direction) do
    parent = Enum.find(state.projects, &(&1.id == child.parent_id))
    children_ids = parent.children_ids
    idx = Enum.find_index(children_ids, &(&1 == child.id))
    swap_idx = if direction == :down, do: idx + 1, else: idx - 1

    if swap_idx < 0 or swap_idx >= length(children_ids) do
      :noreply
    else
      new_children_ids = List.replace_at(children_ids, idx, Enum.at(children_ids, swap_idx))
      new_children_ids = List.replace_at(new_children_ids, swap_idx, child.id)
      updated_parent = %{parent | children_ids: new_children_ids}

      projects = Enum.map(state.projects, fn p -> if p.id == parent.id, do: updated_parent, else: p end)
      Store.save_project(updated_parent, Map.get(state.project_todos, parent.id, []))

      flat = View.flat_ordered_projects(projects)
      new_cursor = (Enum.find_index(flat, &(&1.id == child.id)) || 0) + 1

      {%{state | projects: projects, sidebar_cursor: new_cursor}}
    end
  end
end
