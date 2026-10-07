defmodule OrchidDB.Native do
  @moduledoc false
  @on_load :load
  def load do
    path = :filename.join(:code.priv_dir(:orchiddb), ~c"orchiddb_nif")
    :erlang.load_nif(path, 0)
  end

  def remote_json(_path, _request), do: :erlang.nif_error(:nif_not_loaded)
  def statistics_json(_path, _request), do: :erlang.nif_error(:nif_not_loaded)
  def compile_json(_path, _request), do: :erlang.nif_error(:nif_not_loaded)
end
