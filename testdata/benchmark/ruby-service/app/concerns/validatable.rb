module Concerns
  module Validatable
    def self.included(base)
      base.extend(ClassMethods)
    end

    module ClassMethods
      def validates(*fields)
        @validated_fields = fields
      end

      def validated_fields
        @validated_fields || []
      end
    end

    def valid?
      self.class.validated_fields.all? do |field|
        value = send(field)
        !value.nil? && !value.to_s.empty?
      end
    end
  end
end
