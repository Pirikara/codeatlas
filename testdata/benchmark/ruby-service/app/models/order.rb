require_relative "base"

module Models
  class Order < Base
    attr_reader :user, :items
    attr_accessor :total

    def initialize(user:, items:, **rest)
      super(**rest)
      @user = user
      @items = items
      @total = 0
    end

    def item_count
      items.length
    end

    def summary
      "#{item_count} items, total: #{total}"
    end
  end
end
