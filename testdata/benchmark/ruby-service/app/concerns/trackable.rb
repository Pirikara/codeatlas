module Concerns
  module Trackable
    attr_reader :created_at, :updated_at

    def self.included(base)
      base.extend(ClassMethods)
    end

    module ClassMethods
      def tracked_fields
        [:created_at, :updated_at]
      end
    end

    def touch_timestamps
      now = Time.now
      @created_at ||= now
      @updated_at = now
    end
  end
end
