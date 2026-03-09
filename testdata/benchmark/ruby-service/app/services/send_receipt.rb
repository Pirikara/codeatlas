module Services
  class SendReceipt
    def self.call(user_id:, order: nil, **_rest)
      return { success: false, error: "No order" } unless order

      body = format_receipt(order)
      deliver(user_id, body)
      { success: true }
    end

    def self.format_receipt(order)
      "Order ##{order.id} - Total: #{order.total}"
    end

    def self.deliver(user_id, body)
      # email delivery
    end
  end
end
