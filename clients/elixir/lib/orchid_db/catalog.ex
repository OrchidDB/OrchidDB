defmodule OrchidDB.Catalog do
  @enforce_keys [:endpoint, :scope, :graph]
  defstruct [:endpoint, :scope, :graph, :revision, :library, :auth, token_env: "ORCHID_CATALOG_TOKEN"]

  def new(endpoint, opts) do
    catalog = struct!(__MODULE__, Keyword.put(opts, :endpoint, endpoint))
    if catalog.revision, do: at_revision(catalog, catalog.revision), else: catalog
  end

  def at_revision(catalog, revision) when is_integer(revision) and revision > 0 do
    %{catalog | revision: revision}
  end

  def configuration(catalog) do
    reference = catalog |> Map.from_struct() |> Map.drop([:library, :auth]) |> Map.reject(fn {_, v} -> is_nil(v) end)
    reference = if catalog.auth, do: Map.put(reference, :auth, OrchidDB.CatalogAuth.configuration(catalog.auth)), else: reference
    %{catalog: reference}
  end

  defp command(catalog, action, values \\ %{}) do
    OrchidDB.Internal.Runtime.compile(
      Map.merge(values, %{op: "catalog", action: action, catalog: configuration(catalog).catalog}),
      library: catalog.library
    )
  end

  def discover(catalog, search \\ ""), do: command(catalog, "discover", %{search: search})
  def edges(catalog, search \\ "") do
    with {:ok, result} <- command(catalog, "edges", %{search: search}), do: {:ok, result["objects"]}
  end

  def principals(catalog), do: command(catalog, "principals")
  def principal(catalog, id), do: command(catalog, "principal", %{id: id})
  def register_principal(catalog, id, opts) do
    command(catalog, "register_principal", %{id: id, expected_version: Keyword.get(opts, :expected_version, 0),
      enabled: Keyword.get(opts, :enabled, true), principal: %{subject: id, roles: Keyword.get(opts, :roles, []),
        admin: Keyword.get(opts, :admin, false), tenant: Keyword.get(opts, :tenant)}})
  end
  def grants(catalog), do: command(catalog, "grants")
  def set_grants(catalog, opts) do
    command(catalog, "set_grants", %{expected_version: Keyword.get(opts, :expected_version, 0),
      definition: %{discover: Keyword.get(opts, :discover, []), execute: Keyword.get(opts, :execute, [])}})
  end

  def register_edge(catalog, id, opts) do
    expected = Keyword.get(opts, :expected_version, 0)
    definition = opts |> Keyword.drop([:expected_version, :target_column, :properties]) |> Map.new()
    definition = definition |> Map.put(:kind, "cypher_relationship") |> Map.put_new(:parameters, [])
    definition = Map.put_new(definition, :returns, %{
      target: Keyword.get(opts, :target_column, "target"), properties: Keyword.get(opts, :properties, %{})
    })
    command(catalog, "register_edge", %{id: id, expected_version: expected, definition: definition})
  end

  def object(catalog, id), do: command(catalog, "object", %{id: id})

  def draft(catalog), do: command(catalog, "draft")

  def register_graph(catalog, objects, opts) do
    command(catalog, "register_graph", %{
      expected_version: Keyword.get(opts, :expected_version, 0),
      definition: %{objects: objects, description: Keyword.fetch!(opts, :description),
        execution_connector: Keyword.get(opts, :execution_connector)}
    })
  end

  def publish(catalog, opts) do
    command(catalog, "publish", %{publication: Map.new(opts)})
  end
end
