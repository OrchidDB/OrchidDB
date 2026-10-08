defmodule OrchidDB.Credential do
  @enforce_keys [:source, :value]
  @derive {Inspect, only: [:source]}
  defstruct [:source, :value]
  def value(value), do: %__MODULE__{source: "value", value: value}
  def env(name), do: %__MODULE__{source: "env", value: name}
  def file(path), do: %__MODULE__{source: "file", value: path}
  def configuration(%__MODULE__{} = credential), do: Map.from_struct(credential)
  def configuration(value) when is_binary(value), do: configuration(value(value))
end

defmodule OrchidDB.CatalogAuth do
  @enforce_keys [:config]
  @derive {Inspect, only: []}
  defstruct [:config]
  alias OrchidDB.Credential

  def bearer(token), do: %__MODULE__{config: %{type: "bearer", token: Credential.configuration(token)}}

  def client_credentials(client_id, secret, opts \\ []) do
    %__MODULE__{config: %{type: "client_credentials", client_id: client_id,
      client_secret: Credential.configuration(secret), token_endpoint: Keyword.get(opts, :token_endpoint),
      issuer: Keyword.get(opts, :issuer), scope: Keyword.get(opts, :scope, "PRINCIPAL_ROLE:ALL")}}
  end

  def token_exchange(token, opts \\ []) do
    %__MODULE__{config: %{type: "token_exchange", subject_token: Credential.configuration(token),
      token_endpoint: Keyword.get(opts, :token_endpoint), scope: Keyword.get(opts, :scope, "PRINCIPAL_ROLE:ALL")}}
  end

  def configuration(%__MODULE__{config: config}), do: config
end
