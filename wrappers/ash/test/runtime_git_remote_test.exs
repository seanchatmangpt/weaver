# SPDX-License-Identifier: Apache-2.0

defmodule WeaverAsh.RuntimeGitRemoteTest do
  use ExUnit.Case, async: true
  alias WeaverAsh.{Refusal, Runtime}

  test "git remote registry is refused before execution" do
    assert {:error, %Refusal{type: :remote_registry_not_admitted}} = Runtime.plan("check", "git@example.com:registry.git")
  end
end
