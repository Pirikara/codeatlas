module Validators
  module EmailValidator
    EMAIL_REGEX = /\A[\w+\-.]+@[a-z\d\-]+(\.[a-z\d\-]+)*\.[a-z]+\z/i

    def self.valid?(email)
      return false if email.nil?
      email.match?(EMAIL_REGEX)
    end

    def self.normalize(email)
      email.to_s.strip.downcase
    end
  end
end
