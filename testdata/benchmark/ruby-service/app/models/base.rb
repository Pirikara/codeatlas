require_relative "../concerns/trackable"

module Models
  class Base
    include Concerns::Trackable

    attr_reader :id

    def initialize(id: nil)
      @id = id || generate_id
    end

    def save
      touch_timestamps
      persist
    end

    def self.find(id)
      # stub: database lookup
      nil
    end

    private

    def generate_id
      rand(100_000).to_s
    end

    def persist
      true
    end
  end
end
