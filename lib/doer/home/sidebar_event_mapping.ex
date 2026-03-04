defmodule Doer.Home.SidebarEventMapping do
  alias TermUI.Event

  # Normal mode
  def event_to_msg(%Event.Key{key: key, modifiers: []}, %{sidebar_mode: :normal})
      when key in ["j", :down],
      do: {:msg, :sidebar_down}

  def event_to_msg(%Event.Key{key: key, modifiers: []}, %{sidebar_mode: :normal})
      when key in ["k", :up],
      do: {:msg, :sidebar_up}

  def event_to_msg(%Event.Key{key: "a", modifiers: []}, %{sidebar_mode: :normal}),
    do: {:msg, :sidebar_add_project}

  def event_to_msg(%Event.Key{key: key, modifiers: []}, %{sidebar_mode: :normal})
      when key in ["e", "i"],
      do: {:msg, :sidebar_rename_project}

  def event_to_msg(%Event.Key{key: "d", modifiers: []}, %{sidebar_mode: :normal}),
    do: {:msg, :sidebar_delete_project}

  def event_to_msg(%Event.Key{key: "s", modifiers: []}, %{sidebar_mode: :normal}),
    do: {:msg, :sidebar_add_subproject}

  def event_to_msg(%Event.Key{key: "J", modifiers: []}, %{sidebar_mode: :normal}),
    do: {:msg, :sidebar_reorder_down}

  def event_to_msg(%Event.Key{key: "K", modifiers: []}, %{sidebar_mode: :normal}),
    do: {:msg, :sidebar_reorder_up}

  def event_to_msg(%Event.Key{key: key, modifiers: []}, %{sidebar_mode: :normal})
      when key in [:enter, "l", :right],
      do: {:msg, :sidebar_select}

  # Insert mode
  def event_to_msg(%Event.Key{key: :enter}, %{sidebar_mode: :insert}),
    do: {:msg, :sidebar_confirm_edit}

  def event_to_msg(%Event.Key{key: :escape}, %{sidebar_mode: :insert}),
    do: {:msg, :sidebar_cancel_edit}

  def event_to_msg(%Event.Key{key: :backspace}, %{sidebar_mode: :insert}),
    do: {:msg, :sidebar_backspace}

  def event_to_msg(%Event.Key{key: key}, %{sidebar_mode: :insert})
      when is_binary(key) and byte_size(key) == 1,
      do: {:msg, {:sidebar_type_char, key}}

  # Confirm delete mode
  def event_to_msg(%Event.Key{key: "y", modifiers: []}, %{sidebar_mode: :confirm_delete}),
    do: {:msg, :sidebar_confirm_delete}

  def event_to_msg(%Event.Key{key: key, modifiers: []}, %{sidebar_mode: :confirm_delete})
      when key in ["n", :escape],
      do: {:msg, :sidebar_cancel_delete}

  def event_to_msg(_, _), do: :ignore
end
