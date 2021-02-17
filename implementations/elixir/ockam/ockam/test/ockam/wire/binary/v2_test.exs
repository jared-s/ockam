defmodule Ockam.Wire.Binary.V2.Tests do
  use ExUnit.Case, async: true

  alias Ockam.Wire.Binary.V2
  alias Ockam.Transport.TCPAddress
  alias Ockam.Transport.UDPAddress

  describe "Ockam.Wire.V2" do
    test "encode/1 for TCP" do
      {a,b, c, d} = {127, 0, 0, 1}
      message = %{
        onward_route: [
          %TCPAddress{ip: {a,b, c, d} , port: 4000},
          "printer"
        ],
        return_route: [
          %TCPAddress{ip: {a,b, c, d}, port: 3000}
        ],
        payload: "hello"
      }

      # TODO: embed these in the assert
      _encoded_string_address = <<112, 114, 105, 110, 116, 101, 114>>
      _encoded_payload = <<104, 101, 108, 108, 111>>

      version = 2
      onward_route_size = Map.get(message, :onward_route) |> Enum.count()

      assert <<^version,
        ^onward_route_size,
        1, 7, 0, 127, 0, 0, 1, 160, 15,
          0, 7,
        112, 114, 105, 110, 116, 101, 114,
          1,
        1, 7, 0, 127, 0, 0, 1, 184, 11,
          5,
        104, 101, 108, 108, 111>> = V2.encode(message)
    end

    test "encode/1 for UDP" do
      {a,b, c, d} = {127, 0, 0, 1}
      message = %{
        onward_route: [
          %UDPAddress{ip: {a,b, c, d}, port: 4000},
          "printer"
        ],
        return_route: [
          %UDPAddress{ip: {a,b, c, d}, port: 3000}
        ],
        payload: "hello"
      }

      assert <<2, 2, 2, 9, 2, 7, 0, a, b, c, d, 160, 15, 0, 7, 112, 114, 105, 110, 116, 101, 114, 1, 2, 9, 2, 7, 0, a, b, c, d, 184, 11, 5, 104, 101, 108, 108, 111>> = V2.encode(message)
    end

    test "decode/1 for both" do
      {a,b, c, d} = {127, 0, 0, 1}
      # TODO: make sure this is valid
      encoded = <<2, 2, 2, 9, 2, 7, 0, a, b, c, d, 160, 15, 0, 7, 112, 114, 105, 110, 116, 101, 114, 1, 2, 9, 2, 7, 0, a, b, c, d, 184, 11, 5, 104, 101, 108, 108, 111>>

      assert {:ok, %{
        onward_route: onward_route,
        return_route: return_route,
        payload: payload
      }} = V2.decode(encoded)
      assert [%Ockam.Transport.UDPAddress{ip: {127, 0, 0, 1}, port: 4000}, "printer"] = onward_route
      assert [%Ockam.Transport.UDPAddress{ip: {127, 0, 0, 1}, port: 3000}] = return_route
      assert "hello" = payload
    end
  end
end
