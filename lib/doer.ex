defmodule Doer do
  def run do
    TermUI.Runtime.run(root: Doer.Home)
  end
end
