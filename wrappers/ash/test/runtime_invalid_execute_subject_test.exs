# SPDX-License-Identifier: Apache-2.0

defmodule WeaverAsh.RuntimeInvalidExecuteSubjectTest do
  use ExUnit.Case, async: true
  alias WeaverAsh.{Refusal, Runtime}

  test "execute accepts only an admitted WeaverAsh.Plan" do
    assert {:error, %Refusal{type: :invalid_plan}} = Runtime.execute(%{})
  end
end
