# SPDX-License-Identifier: Apache-2.0

defmodule WeaverAsh.Registry do
  @moduledoc """
  Ash representation of a local Weaver semantic-convention registry.

  The operational identity (`:id`) and RDF subject identity are deliberately
  separate. AshR2RML owns the semantic projection; the Weaver Rust runtime
  remains the registry parser and validator.
  """

  use Ash.Resource,
    data_layer: Ash.DataLayer.Ets,
    extensions: [AshR2RML.Resource]

  attributes do
    uuid_primary_key :id

    attribute :location, :string do
      allow_nil? false
      public? true
    end

    attribute :revision, :string do
      public? true
    end
  end

  r2rml do
    table_name("weaver_registries")
    class("https://opentelemetry.io/weaver/ash#WeaverRegistry")

    subject do
      template("urn:weaver:registry/{id}")
    end

    property(:location, "http://purl.org/dc/terms/source")
    property(:revision, "http://purl.org/dc/terms/identifier")
  end
end

defmodule WeaverAsh.ExecutionReceipt do
  @moduledoc "AshR2RML semantic projection for a receipted Weaver execution."

  use Ash.Resource,
    data_layer: Ash.DataLayer.Ets,
    extensions: [AshR2RML.Resource]

  attributes do
    uuid_primary_key :id

    attribute :operation, :string do
      allow_nil? false
      public? true
    end

    attribute :status, :string do
      allow_nil? false
      public? true
    end

    attribute :exit_status, :integer do
      public? true
    end

    attribute :output_digest, :string do
      allow_nil? false
      public? true
    end

    attribute :subject_digest, :string do
      allow_nil? false
      public? true
    end

    attribute :executable_digest, :string do
      allow_nil? false
      public? true
    end

    attribute :completed_at, :utc_datetime_usec do
      allow_nil? false
      public? true
    end
  end

  r2rml do
    table_name("weaver_execution_receipts")
    class("https://opentelemetry.io/weaver/ash#Receipt")

    subject do
      template("urn:weaver:receipt/{id}")
    end

    property(:operation, "https://opentelemetry.io/weaver/ash#operation")
    property(:status, "https://opentelemetry.io/weaver/ash#standing")
    property(:exit_status, "https://opentelemetry.io/weaver/ash#exitStatus")
    property(:output_digest, "https://opentelemetry.io/weaver/ash#outputDigest")
    property(:subject_digest, "https://opentelemetry.io/weaver/ash#subjectDigest")
    property(:executable_digest, "https://opentelemetry.io/weaver/ash#executableDigest")
    property(:completed_at, "http://www.w3.org/ns/prov#generatedAtTime")
  end
end

defmodule WeaverAsh.Domain do
  @moduledoc "Ash domain for the Weaver semantic control-plane resources."

  use Ash.Domain

  resources do
    resource(WeaverAsh.Registry)
    resource(WeaverAsh.ExecutionReceipt)
  end
end
