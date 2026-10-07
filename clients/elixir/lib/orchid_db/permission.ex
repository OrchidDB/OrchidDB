defmodule OrchidDB.Permission do
  @moduledoc "Provider-neutral helpers for permission relation fields in compiler requests."

  @defaults %{
    "resource_type_column" => "resource_type",
    "permission_column" => "resource_rel",
    "resource_id_column" => "resource_id",
    "subject_type_column" => "subject_type",
    "subject_relation_column" => "subject_rel",
    "subject_id_column" => "subject_id"
  }

  def relation(table, resource_type, permission, columns \\ %{}) when is_map(columns) do
    columns = Map.new(columns, fn {key, value} -> {to_string(key), value} end)

    Map.merge(
      %{"table" => table, "resource_type" => resource_type, "permission" => permission},
      Map.merge(@defaults, columns)
    )
  end

  def scope(resource_column, relation) do
    %{"resource_column" => resource_column, "relation" => relation}
  end

  def authorization(subject_type, subject_id) do
    %{"subject_type" => subject_type, "subject_id" => subject_id}
  end
end
