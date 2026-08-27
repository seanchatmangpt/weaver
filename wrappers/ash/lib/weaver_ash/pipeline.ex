# SPDX-License-Identifier: Apache-2.0

defmodule WeaverAsh.Pipeline.CompileSemantics do
  @moduledoc false
  use Reactor.Step

  @impl Reactor.Step
  def run(_arguments, _context, _options) do
    AshR2RML.compile([WeaverAsh.Registry, WeaverAsh.ExecutionReceipt])
  end

  @impl Reactor.Step
  def compensate(_reason, _arguments, _context, _options), do: :ok
end

defmodule WeaverAsh.Pipeline.Plan do
  @moduledoc false
  use Reactor.Step

  alias WeaverAsh.Runtime

  @impl Reactor.Step
  def run(
        %{semantic_bundle: _bundle, operation: operation, registry: registry, options: options},
        _context,
        _step_options
      ) do
    Runtime.plan(operation, registry, options)
  end

  @impl Reactor.Step
  def compensate(_reason, _arguments, _context, _options), do: :ok
end

defmodule WeaverAsh.Pipeline.Execute do
  @moduledoc false
  use Reactor.Step

  alias WeaverAsh.Runtime

  @impl Reactor.Step
  def run(%{plan: plan}, _context, _options), do: Runtime.execute(plan)

  @impl Reactor.Step
  def compensate(_reason, _arguments, _context, _options) do
    # DO is never retried automatically: a process may already have started.
    :ok
  end
end

defmodule WeaverAsh.Pipeline do
  @moduledoc """
  Reactor graph preserving the wrapper manufacturing law:

      Ash resources -> AshR2RML admission -> plan -> exclusive DO -> receipt

  No generated artifact or semantic derivation receives process authority.
  """

  use Reactor

  input(:operation)
  input(:registry)
  input(:options)

  step :compile_semantics, WeaverAsh.Pipeline.CompileSemantics do
    max_retries(0)
  end

  step :plan, WeaverAsh.Pipeline.Plan do
    argument(:semantic_bundle, result(:compile_semantics))
    argument(:operation, input(:operation))
    argument(:registry, input(:registry))
    argument(:options, input(:options))
    max_retries(0)
  end

  step :execute, WeaverAsh.Pipeline.Execute do
    argument(:plan, result(:plan))
    max_retries(0)
  end

  return(:execute)
end

defmodule WeaverAsh do
  @moduledoc "Public API for the bounded Ash/Reactor control plane around Weaver."

  @spec run(String.t(), String.t(), keyword()) :: {:ok, WeaverAsh.Receipt.t()} | {:error, term()}
  def run(operation, registry, options \\ []) do
    Reactor.run(WeaverAsh.Pipeline, %{
      operation: operation,
      registry: registry,
      options: options
    })
  end
end
