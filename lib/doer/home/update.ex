defmodule Doer.Home.Update do
  alias Doer.{Todo, Store}
  alias Doer.Home.Helpers

  defp save_todos(state) do
    case state.current_view do
      {:project, id} ->
        project = Enum.find(state.projects, &(&1.id == id))
        if project, do: Store.save_project(project, state.todos)
        %{state | project_todos: Map.put(state.project_todos, id, state.todos)}

      :all ->
        ungrouped = Enum.filter(state.todos, &is_nil(&1.source))
        Store.save_all_todos(ungrouped)

        project_groups = state.todos |> Enum.filter(& &1.source) |> Enum.group_by(& &1.source)

        project_todos =
          Enum.reduce(project_groups, state.project_todos, fn {pid, todos}, acc ->
            project = Enum.find(state.projects, &(&1.id == pid))
            if project, do: Store.save_project(project, todos)
            Map.put(acc, pid, todos)
          end)

        # Save empty for projects that had all todos removed
        project_todos =
          Enum.reduce(state.project_todos, project_todos, fn {pid, _}, acc ->
            unless Map.has_key?(project_groups, pid) do
              project = Enum.find(state.projects, &(&1.id == pid))
              if project, do: Store.save_project(project, [])
              Map.put(acc, pid, [])
            else
              acc
            end
          end)

        %{state | project_todos: project_todos}
    end
  end

  def update({:resize, w, h}, state),
    do: {%{state | terminal_width: w, terminal_height: h} |> Helpers.adjust_scroll()}

  def update(:quit, state) do
    state = save_todos(state)
    {state, [:quit]}
  end

  def update(:toggle_sidebar, state) do
    if state.sidebar_open do
      {%{state | sidebar_open: false, focus: :main, sidebar_mode: :normal,
         sidebar_editing_text: "", sidebar_editing_id: nil, sidebar_confirm_project_id: nil}
       |> Helpers.adjust_scroll()}
    else
      {%{state | sidebar_open: true, focus: :sidebar}
       |> Helpers.adjust_scroll()}
    end
  end

  def update(:switch_focus, state) do
    new_focus = if state.focus == :main, do: :sidebar, else: :main
    {%{state | focus: new_focus}}
  end

  def update(:cursor_down, state) do
    max = max(length(Helpers.combined_list(state)) - 1, 0)
    {%{state | cursor: min(state.cursor + 1, max)} |> Helpers.adjust_scroll()}
  end

  def update(:cursor_up, state) do
    {%{state | cursor: max(state.cursor - 1, 0)} |> Helpers.adjust_scroll()}
  end

  def update(:cursor_end, state) do
    max = max(length(Helpers.combined_list(state)) - 1, 0)
    {%{state | cursor: max} |> Helpers.adjust_scroll()}
  end

  def update(:cursor_start, state) do
    {%{state | cursor: 0} |> Helpers.adjust_scroll()}
  end

  def update(:half_page_down, state) do
    jump = div(state.terminal_height, 2)
    max = max(length(Helpers.combined_list(state)) - 1, 0)
    {%{state | cursor: min(state.cursor + jump, max)} |> Helpers.adjust_scroll()}
  end

  def update(:half_page_up, state) do
    jump = div(state.terminal_height, 2)
    {%{state | cursor: max(state.cursor - jump, 0)} |> Helpers.adjust_scroll()}
  end

  # Add todo
  def update(:add_todo, state) do
    source =
      case state.current_view do
        :all ->
          active = Enum.filter(state.todos, &(!&1.done))
          Doer.Home.SidebarUpdate.section_for_cursor(state, min(state.cursor, length(active) - 1))
        {:project, _} -> nil
      end

    new_todo = %{Todo.new("") | source: source}
    active = Enum.filter(state.todos, &(!&1.done))
    insert_pos = min(state.cursor + 1, length(active))

    {before, after_list} = Enum.split(active, insert_pos)
    completed = Enum.filter(state.todos, & &1.done)
    new_todos = before ++ [new_todo] ++ after_list ++ completed

    {%{
       state
       | mode: :insert,
         todos: new_todos,
         cursor: insert_pos,
         editing_id: nil,
         editing_text: "",
         editing_original: ""
     }
     |> Helpers.adjust_scroll()}
  end

  # Edit todo
  def update(:edit_todo, state) do
    combined = Helpers.combined_list(state)

    case Enum.at(combined, state.cursor) do
      nil ->
        :noreply

      todo ->
        {%{
           state
           | mode: :insert,
             editing_id: todo.id,
             editing_text: todo.text,
             editing_original: todo.text
         }}
    end
  end

  # Confirm edit
  def update(:confirm_edit, state) do
    state =
      if state.editing_id == nil do
        if String.trim(state.editing_text) == "" do
          active = Enum.filter(state.todos, &(!&1.done))
          todo = Enum.at(active, state.cursor)
          %{state | todos: Enum.reject(state.todos, &(&1.id == todo.id))}
        else
          active = Enum.filter(state.todos, &(!&1.done))
          todo = Enum.at(active, state.cursor)

          todos =
            Enum.map(state.todos, fn t ->
              if t.id == todo.id, do: %{t | text: state.editing_text}, else: t
            end)

          %{state | todos: todos}
        end
      else
        if String.trim(state.editing_text) == "" do
          %{
            state
            | todos:
                Enum.map(state.todos, fn t ->
                  if t.id == state.editing_id, do: %{t | text: state.editing_original}, else: t
                end)
          }
        else
          %{
            state
            | todos:
                Enum.map(state.todos, fn t ->
                  if t.id == state.editing_id, do: %{t | text: state.editing_text}, else: t
                end)
          }
        end
      end

    cursor = Helpers.clamp_cursor(state.cursor, state.todos)

    state =
      %{
        state
        | mode: :normal,
          cursor: cursor,
          editing_id: nil,
          editing_text: "",
          editing_original: ""
      }
      |> Helpers.adjust_scroll()

    state = save_todos(state)
    {state}
  end

  # Cancel edit
  def update(:cancel_edit, state) do
    state =
      if state.editing_id == nil do
        active = Enum.filter(state.todos, &(!&1.done))
        todo = Enum.at(active, state.cursor)
        if todo, do: %{state | todos: Enum.reject(state.todos, &(&1.id == todo.id))}, else: state
      else
        %{
          state
          | todos:
              Enum.map(state.todos, fn t ->
                if t.id == state.editing_id, do: %{t | text: state.editing_original}, else: t
              end)
        }
      end

    cursor = Helpers.clamp_cursor(state.cursor, state.todos)

    {%{
       state
       | mode: :normal,
         cursor: cursor,
         editing_id: nil,
         editing_text: "",
         editing_original: ""
     }
     |> Helpers.adjust_scroll()}
  end

  def update(:backspace, state) do
    t = String.slice(state.editing_text, 0..-2//1)
    {%{state | editing_text: t}}
  end

  def update({:type_char, char}, state) do
    {%{state | editing_text: state.editing_text <> char}}
  end

  # Delete
  def update(:delete_todo, state) do
    combined = Helpers.combined_list(state)

    case Enum.at(combined, state.cursor) do
      nil ->
        :noreply

      todo ->
        todos = Enum.reject(state.todos, &(&1.id == todo.id))
        cursor = Helpers.clamp_cursor(state.cursor, todos)
        state = save_todos(%{state | todos: todos})
        {%{state | cursor: cursor} |> Helpers.adjust_scroll()}
    end
  end

  # Toggle
  def update(:toggle_todo, state) do
    combined = Helpers.combined_list(state)

    case Enum.at(combined, state.cursor) do
      nil ->
        :noreply

      todo ->
        todos =
          Enum.map(state.todos, fn t ->
            if t.id == todo.id, do: Todo.toggle(t), else: t
          end)

        state = save_todos(%{state | todos: todos})
        {state}
    end
  end

  # Visual mode
  def update(:enter_visual, state),
    do: {%{state | mode: :visual, visual_anchor: state.cursor}}

  def update(:exit_visual, state),
    do: {%{state | mode: :normal}}

  def update(:visual_down, state) do
    max = max(length(Helpers.combined_list(state)) - 1, 0)
    {%{state | cursor: min(state.cursor + 1, max)} |> Helpers.adjust_scroll()}
  end

  def update(:visual_up, state) do
    {%{state | cursor: max(state.cursor - 1, 0)} |> Helpers.adjust_scroll()}
  end

  def update(:delete_selected, state) do
    selected_ids = Helpers.selected_todo_ids(state)
    todos = Enum.reject(state.todos, &(&1.id in selected_ids))
    cursor = Helpers.clamp_cursor(state.cursor, todos)
    state = save_todos(%{state | todos: todos})
    {%{state | mode: :normal, cursor: cursor} |> Helpers.adjust_scroll()}
  end

  def update(:toggle_selected, state) do
    selected_ids = Helpers.selected_todo_ids(state)

    todos =
      Enum.map(state.todos, fn t ->
        if t.id in selected_ids, do: Todo.toggle(t), else: t
      end)

    state = save_todos(%{state | todos: todos})
    {%{state | mode: :normal} |> Helpers.adjust_scroll()}
  end

  def update(:move_selected_down, state) do
    active = Enum.filter(state.todos, &(!&1.done))
    {sel_min, sel_max} = Helpers.selection_range(state)

    if sel_max < length(active) - 1 do
      active_list = Enum.with_index(active)

      {selected, rest} =
        Enum.split_with(active_list, fn {_, i} -> i >= sel_min and i <= sel_max end)

      {before_swap, [swap_item | after_swap]} = Enum.split(rest, sel_min)

      new_active =
        Enum.map(before_swap, &elem(&1, 0)) ++
          [elem(swap_item, 0)] ++
          Enum.map(selected, &elem(&1, 0)) ++
          Enum.map(after_swap, &elem(&1, 0))

      completed = Enum.filter(state.todos, & &1.done)

      {%{
         state
         | todos: new_active ++ completed,
           cursor: state.cursor + 1,
           visual_anchor: state.visual_anchor + 1
       }
       |> Helpers.adjust_scroll()}
    else
      :noreply
    end
  end

  def update(:move_selected_up, state) do
    active = Enum.filter(state.todos, &(!&1.done))
    {sel_min, _sel_max} = Helpers.selection_range(state)

    if sel_min > 0 do
      active_list = Enum.with_index(active)

      {selected, rest} =
        Enum.split_with(active_list, fn {_, i} ->
          i >= sel_min and i <= elem(Helpers.selection_range(state), 1)
        end)

      {before, after_list} = Enum.split(rest, sel_min - 1)

      new_active =
        Enum.map(before, &elem(&1, 0)) ++
          Enum.map(selected, &elem(&1, 0)) ++
          Enum.map(after_list, &elem(&1, 0))

      completed = Enum.filter(state.todos, & &1.done)

      {%{
         state
         | todos: new_active ++ completed,
           cursor: state.cursor - 1,
           visual_anchor: state.visual_anchor - 1
       }
       |> Helpers.adjust_scroll()}
    else
      :noreply
    end
  end

  # Sidebar messages — delegate to SidebarUpdate
  def update(msg, state) when msg in [
    :sidebar_down, :sidebar_up, :sidebar_select,
    :sidebar_add_project, :sidebar_add_subproject,
    :sidebar_rename_project, :sidebar_delete_project,
    :sidebar_confirm_delete, :sidebar_cancel_delete,
    :sidebar_confirm_edit, :sidebar_cancel_edit,
    :sidebar_backspace,
    :sidebar_reorder_down, :sidebar_reorder_up
  ] do
    Doer.Home.SidebarUpdate.update(msg, state)
  end

  def update({:sidebar_type_char, _} = msg, state),
    do: Doer.Home.SidebarUpdate.update(msg, state)

  # Search
  def update(:enter_search, state),
    do: {%{state | mode: :search, search_text: state.search_text}}

  def update(:confirm_search, state) do
    matches = Helpers.filter_todos(state.todos, state.search_text)

    {%{state | mode: :search_nav, search_matches: Enum.map(matches, & &1.id), cursor: 0}
     |> Helpers.adjust_scroll()}
  end

  def update(:cancel_search, state),
    do: {%{state | mode: :normal, search_text: "", search_matches: []}}

  def update(:search_backspace, state) do
    t = String.slice(state.search_text, 0..-2//1)
    {%{state | search_text: t}}
  end

  def update({:search_type, char}, state),
    do: {%{state | search_text: state.search_text <> char}}

  # Help
  def update(:toggle_help, state),
    do: {%{state | show_help: !state.show_help}}
end
