# SPDX-License-Identifier: Apache-2.0

defmodule WeaverAsh.RuntimeBlankIdTest do
  use ExUnit.Case, async: true
  alias WeaverAsh.{Refusal, Runtime}

  test "blank package id is refused before execution" do
    assert {:error, %Refusal{type: :invalid_id}} = Runtime.plan("package_json", "fixtures/registry", id: "")
  end
end
