require_relative "base"
require_relative "../concerns/validatable"

module Models
  class User < Base
    include Concerns::Validatable

    attr_reader :name, :email
    attr_accessor :last_order_at

    validates :name, :email

    def initialize(name:, email:, **rest)
      super(**rest)
      @name = name
      @email = email
    end

    def full_name
      name.capitalize
    end

    def active?
      !last_order_at.nil?
    end

    def self.find(id)
      # stub
      new(name: "user_#{id}", email: "user_#{id}@example.com", id: id)
    end
  end
end
