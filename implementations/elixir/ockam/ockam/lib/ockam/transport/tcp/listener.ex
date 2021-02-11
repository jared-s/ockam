if Code.ensure_loaded?(:ranch) do
  defmodule Ockam.Transport.TCP.Listener do
    @moduledoc false

    use Ockam.Worker
    alias Ockam.Transport.TCPAddress
    alias Ockam.Message
    alias Ockam.Wire

    @tcp 1
    # TODO: modify this for tcp
    @wire_encoder_decoder Ockam.Wire.Binary.V1

    @doc false
    @impl true
    def setup(options, state) do
      ip = Keyword.get_lazy(options, :ip, &default_ip/0)
      state = Map.put(state, :ip, ip)

      port = Keyword.get_lazy(options, :port, &default_port/0)
      state = Map.put(state, :port, port)

      route_outgoing = Keyword.get(options, :route_outgoing, false)

      ref = make_ref()
      transport = :ranch_tcp
      transport_options = [port: port]
      protocol = __MODULE__.Handler
      protocol_options = []

      with {:ok, _apps} <- Application.ensure_all_started(:ranch),
           :ok <- start_listener(ref, transport, transport_options, protocol, protocol_options),
           :ok <- setup_routed_message_handler(route_outgoing, state.address) do
        {:ok, state}
      end
    end

    defp start_listener(ref, transport, transport_options, protocol, protocol_options) do
      r = :ranch.start_listener(ref, transport, transport_options, protocol, protocol_options)

      case r do
        {:ok, _child} -> :ok
        {:ok, _child, _info} -> :ok
        {:error, reason} -> {:error, {:could_not_start_ranch_listener, reason}}
      end
    end

    defp setup_routed_message_handler(true, listener) do
      handler = fn message -> handle_routed_message(listener, message) end

      with :ok <- Router.set_message_handler(@tcp, handler),
           :ok <- Router.set_message_handler(Ockam.Transport.TCPAddress, handler) do
        :ok
      end
    end

    defp setup_routed_message_handler(_something_else, _listener), do: :ok

    defp handle_routed_message(listener, message) do
      Node.send(listener, message)
    end

    @impl true
    def handle_message({:tcp, _socket, _from_ip, _from_port, _packet} = tcp_message, state) do
      send_over_tcp(tcp_message, state.address)
      {:ok, state}
    end

    def handle_message(message, state) do
      encode_and_send_over_tcp(message, state)
      {:ok, state}
    end

    defp encode_and_send_over_tcp(message, state) do
      message = create_outgoing_message(message)

      with {:ok, destination, message} <- pick_destination_and_set_onward_route(message, state.address),
           {:ok, message} <- set_return_route(message, state.address),
           {:ok, encoded_message} <- Wire.encode(@wire_encoder_decoder, message),
           {:ok, encoded_message_with_length_prepended} <- prepend_varint_length(encoded_message),
           :ok <- send_over_tcp(encoded_message_with_length_prepended, destination) do
        :ok
      end
    end

    defp prepend_varint_length(message) do
      bytesize = IO.iodata_length(message)
      case Ockam.Wire.Binary.VarInt.encode(bytesize) do
        {:error, reason} -> {:error, reason}
        varint_length -> {:ok, [varint_length, message]}
      end
    end

    defp send_over_tcp(_message,address) do
      IO.inspect(address, label: "send_over_tcp")
      :ok
    end


    defp create_outgoing_message(message) do
      %{
        onward_route: Message.onward_route(message),
        return_route: Message.return_route(message),
        payload: Message.payload(message)
      }
    end

    defp pick_destination_and_set_onward_route(message, address) do
      destination_and_onward_route =
        message
        |> Message.onward_route()
        |> Enum.drop_while(fn a -> a === address end)
        |> List.pop_at(0)

      case destination_and_onward_route do
        {nil, []} -> {:error, :no_destination}
        {%TCPAddress{} = destination, r} -> {:ok, destination, %{message | onward_route: r}}
        {{@tcp, address}, onward_route} -> deserialize_address(message, address, onward_route)
        {destination, _onward_route} -> {:error, {:invalid_destination, destination}}
      end
    end

    defp deserialize_address(message, address, onward_route) do
      case TCPAddress.deserialize(address) do
        {:error, error} -> {:error, error}
        destination -> {:ok, destination, %{message | onward_route: onward_route}}
      end
    end

    defp set_return_route(%{return_route: return_route} = message, address) do
      {:ok, %{message | return_route: [address | return_route]}}
    end


    defp default_ip, do: {127, 0, 0, 1}
    defp default_port, do: 4000
  end

  defmodule Ockam.Transport.TCP.Listener.Handler do
    @moduledoc false

    def start_link(ref, socket, transport, _opts) do
      pid = :proc_lib.spawn_link(__MODULE__, :init, [ref, socket, transport])
      {:ok, pid}
    end

    def init(ref, socket, transport) do
      {:ok, _} = :ranch.handshake(ref)
      :ok = transport.setopts(socket, [{:active, true}, {:nodelay, true}, {:reuseaddr, true}])
      :gen_server.enter_loop(__MODULE__, [], %{socket: socket, transport: transport})
    end

    def handle_info({:tcp, socket, data}, %{socket: socket, transport: _transport} = state) do
      # this will repeatedly try to decode the length even if the decoding succeeds.
      # TODO: we should probably only decode the length once
      with {bytesize, rest} <- Ockam.Wire.Binary.VarInt.decode(data),
          {:ok, _data} <- check_length(data, bytesize) do
          send_to_router(rest)
        else
            {:error, %Ockam.Wire.DecodeError{}} -> enqueue_data(data)
            :not_enough_data -> enqueue_data(data)
        end
      IO.puts("#{inspect(data)}")
      # @TODO: do something other than echo
      # transport.send(socket, data)
      {:noreply, state}
    end

    def handle_info({:tcp_closed, socket}, %{socket: socket, transport: transport} = state) do
      IO.puts("Closing")
      transport.close(socket)
      {:stop, :normal, state}
    end

    defp check_length(data, size) do
      case IO.iodata_length(data) == size do
        true -> :ok
        false -> :not_enough_data
      end
    end

    defp enqueue_data(_data) do
      # TODO: do this
    end

    defp send_to_router(_message) do
      # TODO: do this
    end
  end
end
