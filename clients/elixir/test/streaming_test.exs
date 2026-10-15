defmodule OrchidDB.StreamingTest do
  use ExUnit.Case
  alias OrchidDB.StreamConnection, as: Stream
  @tag :streaming
  test "Kafka routes insertion and read checks through the application's SpiceDB client" do
    if path = System.get_env("ORCHIDDB_STREAM_FIXTURE") do
      :inets.start()
      f = path |> File.read!() |> Jason.decode!()

      auth = fn subject, checks ->
        body = Jason.encode!(%{subject: subject, checks: checks})

        {:ok, {{_, 200, _}, _, response}} =
          :httpc.request(
            :post,
            {String.to_charlist(f["auth_url"]), [], ~c"application/json", body},
            [{:timeout, 10_000}],
            []
          )

        Jason.decode!(to_string(response))
      end

      options = %{kafka: f["kafka"], topic: f["topic_prefix"] <> "-elixir", source: f["source"]}
      {:ok, c} = Stream.start_link(f["schema"], options, auth)

      change = fn key, version ->
        %{table: "docs", key: key, version: version, data: %{name: "#{key}-#{version}"}}
      end

      try do
        assert {:error, error} = Stream.insert(c, change.("blocked", 1), f["subject"])
        assert error =~ "denied"
        assert {:ok, _} = Stream.insert(c, change.("a", 1), f["subject"])

        assert {:ok, _} =
                 Stream.insert_batch(
                   c,
                   [change.("b", 1), change.("hidden", 1), change.("a", 2)],
                   f["subject"]
                 )

        assert {:ok, query} = Stream.query(c, f["query"], f["subject"])

        snapshot = %{
          source: f["source"],
          offsets: f["offsets"],
          changes: [change.("a", 1), change.("b", 1)]
        }

        assert {:ok, hybrid} = Stream.query_hybrid(c, f["query"], f["subject"], snapshot)
        assert {:ok, _} = Stream.subscribe(c, f["stream_query"], f["subject"], f["offsets"])

        {frames, offsets} =
          Enum.reduce_while(1..20, {[], %{}}, fn _, {frames, _} ->
            {:ok, b} = Stream.next_batch(c, 16, 100)
            result = {frames ++ b["frames"], b["offsets"]}
            if b["offsets"]["0"] == 4, do: {:halt, result}, else: {:cont, result}
          end)

        assert length(frames) == 3
        assert offsets == %{"0" => 4}
        assert {:ok, _} = Stream.unsubscribe(c)

        File.write!(
          "/tmp/orchid-stream-elixir-result.json",
          Jason.encode!(%{query: query, hybrid: hybrid, frames: frames, offsets: offsets})
        )
      after
        Stream.close(c)
      end
    end
  end
end
